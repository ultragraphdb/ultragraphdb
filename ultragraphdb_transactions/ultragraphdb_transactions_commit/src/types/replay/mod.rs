/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{SessionToken, SystemId, Version};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::{
    RecordKind, RecordOperation, RoundHeader, RoundView, WriteSetRecordView,
};
use ultragraphdb_store_apply::{
    Applied, ChangesetApplier, ChangesetError, DropTarget, Executed, LogicalQuad, LogicalWrites,
};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_log::{OrderedLog, Position, Round};
use ultragraphdb_traits_store::{Executor, ListCache, Store, VersionlessEntry};

use crate::errors::recovery_error::RecoveryError;
use crate::types::sessions::Sessions;
use crate::types::stored_reply::StoredReply;

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
    /// Replays `log` into the store of `applier`, as a partition does when it starts and before it accepts a request
    /// (`PROTOCOL.md` §12.2).
    ///
    /// Every round above the store's applied log position ([`Replay::applied_position`]) is replayed in order, each as one round of the store at its
    /// versions through the derivation it names: each committed record's quads, statement records and drop with every
    /// key they derive, a build's end, the round's dictionary entries, and the session table's entries its committed
    /// requests leave, with their stored replies, on a table of `capacity` sessions. Replaying a round the store holds
    /// writes what it holds.
    ///
    /// # Errors
    ///
    /// [`RecoveryError::behind_checkpoint`] for a store whose applied position is below the log's checkpoint, naming
    /// both; [`RecoveryError::store_not_in_log`] for one beyond the log's tail, naming it; [`RecoveryError::gap`] naming
    /// the first position missing between them; [`RecoveryError::derivation`] for a round naming a derivation
    /// version the store does not know, applying nothing of it; and the log's, a round's, the session table's and the
    /// store's errors.
    pub async fn run<L, S, C, E, B>(
        applier: &ChangesetApplier<S, C, E, B>,
        log: &mut L,
        capacity: usize,
    ) -> Result<Self, RecoveryError>
    where
        L: OrderedLog,
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        Self::run_in(0, applier, log, capacity).await
    }

    /// Replays `log` as [`Replay::run`] does, for partition `partition`, which the session tokens of the stored replies
    /// it rebuilds name.
    ///
    /// # Errors
    ///
    /// As [`Replay::run`].
    pub async fn run_in<L, S, C, E, B>(
        partition: u32,
        applier: &ChangesetApplier<S, C, E, B>,
        log: &mut L,
        capacity: usize,
    ) -> Result<Self, RecoveryError>
    where
        L: OrderedLog,
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let store = applier.quads().store();
        let applied = Self::applied_position(store)?.get();
        let checkpoint = log.checkpoint().get();
        if applied < checkpoint {
            return Err(RecoveryError::behind_checkpoint(applied, checkpoint));
        }
        let (tail, _) = log.tail();
        if applied > tail.get() {
            return Err(RecoveryError::store_not_in_log(applied));
        }
        let mut sessions = Sessions::load(store)
            .map_err(RecoveryError::sessions)?
            .bounded(capacity);
        let mut replay = Self {
            rounds: 0,
            applied: applier.quads().lists().applied_ts(),
        };
        let mut next = applied.saturating_add(1);
        // Each turn reads the rounds from the next position, at least one or none, so the scan ends at the log's tail,
        // which a log holding finitely many rounds has.
        loop {
            let rounds = log
                .read(Position::new(next), READ_BYTES)
                .await
                .map_err(RecoveryError::log)?;
            if rounds.is_empty() {
                break;
            }
            for round in &rounds {
                let position = round.position().get();
                if position != next {
                    return Err(RecoveryError::gap(next));
                }
                if round.derivation().get() != LogicalQuad::DERIVATION {
                    return Err(RecoveryError::derivation(
                        position,
                        round.derivation().get(),
                    ));
                }
                let view = round_view(round)?;
                replay.round(partition, applier, &mut sessions, &view, position)?;
                next = position.saturating_add(1);
            }
        }
        replay.applied = applier.quads().lists().applied_ts();
        Ok(replay)
    }

    /// Replays one round at `position` into the store, its dictionary entries, the session table's entries it changes
    /// and its applied position in its batch.
    fn round<S, C, E, B>(
        &mut self,
        partition: u32,
        applier: &ChangesetApplier<S, C, E, B>,
        sessions: &mut Sessions,
        view: &RoundView<'_>,
        position: u64,
    ) -> Result<(), RecoveryError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let apply = |cause| RecoveryError::apply(position, cause);
        let mut system: Vec<VersionlessEntry> = view
            .dictionary()
            .iter()
            .filter_map(|e| Some(VersionlessEntry::put(e.key().to_vec(), e.value()?.to_vec())))
            .collect();
        let dictionary: Vec<VersionlessEntry> = system.clone();
        let inherited: Vec<&VersionlessEntry> = dictionary.iter().collect();
        let mut executions: Vec<(u64, Executed)> = Vec::new();
        let mut through = applier.quads().lists().applied_ts();
        for (k, record) in view.records().iter().enumerate() {
            let Some(version) = view.commit_version(k) else {
                continue;
            };
            through = version;
            if let Some(executed) = prepare(applier, record, &inherited).map_err(apply)? {
                executions.push((version, executed));
            }
            let session = u128::from_be_bytes(record.session());
            if record.kind() == RecordKind::Transaction && session != 0 {
                let token =
                    SessionToken::new(partition, position, Version::from_u64(version).epoch());
                let results = (0..record.result_count())
                    .filter_map(|i| record.result(i))
                    .collect();
                let reply =
                    StoredReply::new(version, token, results, record.result_terms().to_table());
                sessions.committed(session, record.request(), reply);
            }
        }
        sessions.drain_entries(&mut system);
        system.push(VersionlessEntry::put(
            Self::APPLIED_POSITION_KEY.to_vec(),
            position.to_le_bytes().to_vec(),
        ));
        let _applied: Vec<Applied> = applier
            .apply_round_with(executions, &system, through)
            .map_err(apply)?;
        applier.forget_catalogs();
        self.rounds += 1;
        Ok(())
    }

    /// The key of the store's applied log position, under the system prefix, outside every namespace: its 8 bytes,
    /// little-endian, written with every round's batch.
    pub const APPLIED_POSITION_KEY: &'static [u8] = b"\xffultragraphdb/applied_position";

    /// The store's applied log position: the position of the last round it applied, which every round's batch records
    /// with its data; [`Position::ZERO`] for a store that applied none.
    ///
    /// # Errors
    ///
    /// [`RecoveryError::applied_position`] for the store's error reading it, or its corruption error for a value that is
    /// not 8 bytes.
    pub fn applied_position<S: Store>(store: &S) -> Result<Position, RecoveryError> {
        let Some(bytes) = store
            .get_versionless(Self::APPLIED_POSITION_KEY)
            .map_err(RecoveryError::applied_position)?
        else {
            return Ok(Position::ZERO);
        };
        let position = <[u8; 8]>::try_from(bytes.as_slice()).map_err(|_| {
            RecoveryError::applied_position(StoreError::corrupted(
                Self::APPLIED_POSITION_KEY.to_vec(),
                "an applied log position that is not 8 bytes".to_string(),
            ))
        })?;
        Ok(Position::new(u64::from_le_bytes(position)))
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
        round.derivation(),
    );
    RoundView::from_body(header, round.body())
        .map_err(|cause| RecoveryError::round(round.position().get(), cause))
}

/// The execution deriving `record`'s writes, `None` for a record whose keys this stage does not derive on replay: a
/// registration a home accepted, whose referrer set is the partitions' (group 7), and a build's record (group 6).
fn prepare<S, C, E, B>(
    applier: &ChangesetApplier<S, C, E, B>,
    record: &WriteSetRecordView<'_>,
    dictionary: &[&VersionlessEntry],
) -> Result<Option<Executed>, ChangesetError>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    if record.kind() != RecordKind::Transaction {
        return Ok(None);
    }
    let namespace = record.namespace();
    if let Some(target) = record.operation(0).and_then(drop_target) {
        return applier.prepare_replayed_drop(namespace, target).map(Some);
    }
    let quads = |values: ultragraphdb_protocol_wire::KeyValues<'_>| -> Result<Vec<LogicalQuad>, ChangesetError> {
        values
            .iter()
            .map(|e| LogicalQuad::decode(e.key(), e.value().unwrap_or_default()))
            .collect()
    };
    let records = record
        .statement_records()
        .iter()
        .map(|e| statement_record(e.key(), e.value()))
        .collect::<Result<Vec<_>, ChangesetError>>()?;
    let writes = LogicalWrites::new(quads(record.inserted())?, quads(record.removed())?, records);
    applier
        .prepare_logical(namespace, &writes, dictionary)
        .map(Some)
}

/// A statement record's graph, statement ID and value from its record entry: a graph's 16 bytes and the ID's.
fn statement_record(
    key: &[u8],
    value: Option<&[u8]>,
) -> Result<(Graph, SystemId, Option<Vec<u8>>), ChangesetError> {
    let corrupted = || -> ChangesetError {
        StoreError::corrupted(
            key.to_vec(),
            "a logged statement record's key is no graph and ID".to_string(),
        )
        .into()
    };
    let (graph, id) = key.split_at_checked(16).ok_or_else(corrupted)?;
    let id = SystemId::decode(id).map_err(|_| corrupted())?;
    let graph = if graph.iter().all(|b| *b == 0) {
        Graph::Default
    } else {
        Graph::Named(SystemId::decode(graph).map_err(|_| corrupted())?)
    };
    Ok((graph, id, value.map(<[u8]>::to_vec)))
}

/// The drop a record's operation names, `None` for a build's.
fn drop_target(operation: RecordOperation) -> Option<DropTarget> {
    match operation {
        RecordOperation::DropPredicate(id) => Some(DropTarget::Predicate(id)),
        RecordOperation::DropGraph(id) => Some(DropTarget::Graph(Graph::Named(id))),
        RecordOperation::DropNamespaceData => Some(DropTarget::Data),
        RecordOperation::DropNamespace => Some(DropTarget::Namespace),
        RecordOperation::DropAll => Some(DropTarget::All),
        RecordOperation::StartBuild(_) | RecordOperation::CompleteBuild(_) => None,
    }
}
