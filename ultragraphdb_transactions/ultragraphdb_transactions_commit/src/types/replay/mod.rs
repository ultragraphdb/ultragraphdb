/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_protocol_wire::{
    RecordEntry, RecordOperation, RoundHeader, RoundView, WriteSetRecordView,
};
use ultragraphdb_store_apply::{BuildEnd, ChangesetApplier, DropTarget, Executed, RecordWrites};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_log::{OrderedLog, Position, Round};
use ultragraphdb_traits_store::{Executor, ListCache, Store, VersionlessEntry};

use crate::errors::recovery_error::RecoveryError;
use crate::types::sessions::Sessions;

/// The bytes of rounds one read of the log returns at most.
const READ_BYTES: u32 = 1 << 24;

/// What recovery replayed (7.9): the number of rounds, and the store's applied version after them, the last commit
/// version of the log's last round that committed anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Replay {
    rounds: usize,
    applied: u64,
}

impl Replay {
    /// Replays `log`, from `from`, the position after its checkpoint, into the store of `applier`, as a node does when
    /// it starts and before it accepts a connection (`PROTOCOL.md` §12.2, decided 2026-10-28).
    ///
    /// The store's applied version is the last commit version of a round the log holds, or 0, and every round after
    /// it is replayed in order, each as one round of the store at its versions: each committed record's writes, its
    /// operation, a drop, or a build's end, its build restored from the declaration the record names, and the session
    /// table's entries the round changes, a registration or a committed request with its stored reply, on a table of
    /// `capacity` sessions, the bound the node's pipeline runs with. A swap or undo whose predicate the store no longer
    /// declares writes nothing, as its first apply found the build a drop cancelled: no record between the drop and the
    /// end declares the predicate again (`PROTOCOL.md` §12.2). Replaying a round the store holds writes what it holds.
    ///
    /// # Errors
    ///
    /// [`RecoveryError::StoreNotInLog`] when the store's applied version is neither 0 nor a held round's last version,
    /// unless it lies below the first round held after a checkpoint, which the checkpoint covers;
    /// [`RecoveryError::Gap`] for an empty store and a trimmed log; [`RecoveryError::BehindCheckpoint`] for a store
    /// below the last commit version the log's checkpoint covers; and the log's, a round's, the session table's and
    /// the store's errors.
    pub async fn run<L, S, C, E>(
        applier: &ChangesetApplier<S, C, E>,
        log: &mut L,
        from: Position,
        capacity: usize,
    ) -> Result<Self, RecoveryError>
    where
        L: OrderedLog,
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        E: Executor,
    {
        let applied = applier.quads().lists().applied_ts();
        let trimmed = from.get() > 1;
        if applied == 0 && trimmed {
            return Err(RecoveryError::Gap(from.get() - 1));
        }
        let checkpoint = log.checkpoint_version();
        if applied < checkpoint {
            return Err(RecoveryError::BehindCheckpoint(applied, checkpoint));
        }
        let mut sessions = Sessions::load(applier.quads().store())
            .map_err(RecoveryError::Sessions)?
            .bounded(capacity);
        let mut replay = Self { rounds: 0, applied };
        // Whether the store's applied version was found: 0 always is, and below the first round after a checkpoint.
        let mut found = applied == 0;
        let mut first = true;
        let mut at = from;
        loop {
            let rounds = log.read(at, READ_BYTES).await.map_err(RecoveryError::Log)?;
            let Some(last) = rounds.last() else {
                break;
            };
            at = Position::new(last.position().get().saturating_add(1));
            for round in &rounds {
                let view = round_view(round)?;
                let Some(last_version) = last_version(&view) else {
                    continue;
                };
                if last_version <= applied {
                    found |= last_version == applied;
                    first = false;
                    continue;
                }
                if !found && !(first && trimmed && applied < view.header().first_version()) {
                    return Err(RecoveryError::StoreNotInLog(applied));
                }
                found = true;
                first = false;
                replay.round(applier, &mut sessions, &view, round)?;
            }
        }
        if !found && !(first && trimmed) {
            return Err(RecoveryError::StoreNotInLog(applied));
        }
        applier.forget_catalogs();
        replay.applied = applier.quads().lists().applied_ts();
        Ok(replay)
    }

    /// Replays one round into the store, with the session table's entries it changes.
    fn round<S, C, E>(
        &mut self,
        applier: &ChangesetApplier<S, C, E>,
        sessions: &mut Sessions,
        view: &RoundView<'_>,
        round: &Round,
    ) -> Result<(), RecoveryError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        E: Executor,
    {
        let position = round.position().get();
        let apply = |cause| RecoveryError::Apply(position, cause);
        let mut executions: Vec<(u64, Executed)> = Vec::new();
        let mut through = 0;
        for (k, record) in view.records().iter().enumerate() {
            let Some(version) = view.commit_version(k) else {
                continue;
            };
            through = version;
            let (namespace, session, request) =
                (record.namespace(), record.session(), record.request());
            if session != 0 && request == 0 {
                sessions.register(session, record.principal());
                continue;
            }
            let executed = match record.operation() {
                Some(RecordOperation::DropPredicate(uid)) => applier
                    .prepare_replayed_drop(namespace, DropTarget::Predicate(uid))
                    .map_err(apply)?,
                Some(RecordOperation::SwapBuild(id)) => {
                    end_build(applier, record, BuildEnd::Swap(id)).map_err(apply)?
                }
                Some(RecordOperation::UndoBuild(id)) => {
                    end_build(applier, record, BuildEnd::Undo(id)).map_err(apply)?
                }
                Some(RecordOperation::DropGraph(uid)) => applier
                    .prepare_replayed_drop(namespace, DropTarget::Graph(uid))
                    .map_err(apply)?,
                Some(RecordOperation::DropNamespaceData) => applier
                    .prepare_replayed_drop(namespace, DropTarget::Data)
                    .map_err(apply)?,
                Some(RecordOperation::DropNamespace) => applier
                    .prepare_replayed_drop(namespace, DropTarget::Namespace)
                    .map_err(apply)?,
                Some(RecordOperation::DropAll) => applier
                    .prepare_replayed_drop(namespace, DropTarget::All)
                    .map_err(apply)?,
                None => applier
                    .prepare_replay(namespace, record_writes(record))
                    .map_err(apply)?,
            };
            if session != 0 {
                let results = (0..record.result_count())
                    .filter_map(|i| record.result(i))
                    .collect();
                sessions.committed(session, request, version, results);
            }
            executions.push((version, executed));
        }
        let mut system: Vec<VersionlessEntry> = Vec::new();
        sessions.drain_entries(&mut system);
        applier
            .apply_round_with(executions, &system, through)
            .map_err(apply)?;
        applier.forget_catalogs();
        self.rounds += 1;
        Ok(())
    }

    /// The number of rounds replayed.
    pub fn rounds(&self) -> usize {
        self.rounds
    }

    /// The store's applied version after the replay.
    pub fn applied(&self) -> u64 {
        self.applied
    }
}

/// The view of `round`, its header rebuilt from what the log keeps.
fn round_view(round: &Round) -> Result<RoundView<'_>, RecoveryError> {
    let header = RoundHeader::new(
        round.position().get(),
        round.parent(),
        0,
        round.first_version(),
        round.transactions(),
    );
    RoundView::from_body(header, round.body())
        .map_err(|cause| RecoveryError::Round(round.position().get(), cause))
}

/// The last commit version of `view`, `None` for a round that committed nothing.
fn last_version(view: &RoundView<'_>) -> Option<u64> {
    view.records()
        .len()
        .checked_sub(1)
        .and_then(|k| view.commit_version(k))
}

/// The proposal of a logged build's end, its build restored from the declaration its record names when the store
/// still declares the predicate.
fn end_build<S, C, E>(
    applier: &ChangesetApplier<S, C, E>,
    record: &WriteSetRecordView<'_>,
    end: BuildEnd,
) -> Result<Executed, ultragraphdb_store_apply::ChangesetError>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor,
{
    if let Some(entry) = record.catalog().iter().next()
        && let (Ok((QuadKey::Catalog { namespace, .. }, _)), Some(value)) =
            (QuadKey::parse(entry.key()), entry.value())
    {
        applier.restore_build(namespace, end, entry.key(), value)?;
    }
    applier.prepare_build_end(end)
}

/// The writes of `record`'s sections 0 to 4.
fn record_writes<'r>(record: &WriteSetRecordView<'r>) -> RecordWrites<'r> {
    let pairs =
        |entries: ultragraphdb_protocol_wire::KeyValues<'r>| -> Vec<(&'r [u8], Option<&'r [u8]>)> {
            entries
                .iter()
                .map(|e: RecordEntry<'r>| (e.key(), e.value()))
                .collect()
        };
    RecordWrites::new(
        record
            .list_deltas()
            .iter()
            .map(|e| (e.key(), e.value().unwrap_or_default()))
            .collect(),
        pairs(record.dictionary()),
        pairs(record.statements()),
        pairs(record.catalog()),
        pairs(record.versionless()),
    )
}
