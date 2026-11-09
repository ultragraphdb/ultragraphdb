/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use ultragraphdb_cluster_sequencer::{CatalogVersion, ClockReading, Close};
use ultragraphdb_common_service::InboxTurn;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};
use ultragraphdb_traits_log::OrderedLog;

use crate::types::asked::Asked;
use crate::types::owner::owner_syncing::{SyncReads, while_syncing};
use crate::types::owner::{
    ASKS_PER_TURN_MAX, CLOSES_PER_SNAPSHOT, Durable, METADATA_SNAPSHOT, Owner, SEQUENCER_SNAPSHOT,
    stopped,
};
use crate::utils::snapshot_file;

impl<M: OrderedLog, S: OrderedLog> Owner<M, S> {
    /// Runs the owner until every [`crate::OwnerHandle`] is gone: each turn takes what has been asked, at most
    /// `ASKS_PER_TURN_MAX` past the ask it waited for or past those the last turn's syncs kept, at most `ASKS_PER_TURN_MAX`
    /// a sync, then answers what its sync made durable and closes an epoch that is due, by `clock`.
    ///
    /// A dropped future drops the owner: every ask waiting on it is answered as the owner stopped, what its logs hold
    /// unsynced is durable or lost as the process's end leaves it, and the next start recovers from what is durable.
    ///
    /// # Errors
    ///
    /// [`RoleError::io`] for a failed log or snapshot file, on which the owner stops; [`RoleError::software`] for a
    /// broken invariant.
    pub async fn run(
        mut self,
        mut inbox: UnboundedReceiver<Asked>,
        clock: impl Fn() -> ClockReading,
    ) -> Result<(), RoleError> {
        loop {
            if self.waiting.is_empty() {
                let wait = self
                    .sequencer
                    .deadline()
                    .saturating_sub(clock().monotonic_us());
                let wait = std::time::Duration::from_micros(wait);
                // A message waiting stays in the channel when the wait ends first, so nothing is lost by dropping the wait.
                if let Ok(asked) = compio::time::timeout(wait, inbox.next()).await {
                    let Some(first) = asked else {
                        return Ok(());
                    };
                    self.take(first)?;
                    self.take_turn(&mut inbox)?;
                }
            } else {
                // What the last sync's wait took and kept comes first, in its order, then what came after it.
                while let Some(kept) = self.waiting.pop_front() {
                    self.take(kept)?;
                }
                self.take_turn(&mut inbox)?;
            }
            self.turn(clock(), &mut inbox).await?;
        }
    }

    /// Takes what waits in `inbox`, at most `ASKS_PER_TURN_MAX`, the rest waiting for the next turn (Ultra rule 2).
    fn take_turn(&mut self, inbox: &mut UnboundedReceiver<Asked>) -> Result<(), RoleError> {
        let mut taking = InboxTurn::new(ASKS_PER_TURN_MAX);
        while let Some(next) = taking.receive(|| inbox.try_recv()) {
            self.take(next)?;
        }
        Ok(())
    }

    /// One turn: the answers the asks of this turn wait for, then the close due at `now`, durable in the sequencer's log
    /// before the group learns it.
    /// A future dropped at a sync leaves its answers unsent and the owner fit only to be dropped, as [`Owner::run`]
    /// says.
    async fn turn(
        &mut self,
        now: ClockReading,
        inbox: &mut UnboundedReceiver<Asked>,
    ) -> Result<(), RoleError> {
        self.answer_durable(inbox).await?;
        // A report just durable may end a check, whose restoring version takes the next sync.
        self.run_checks(now.monotonic_us())?;
        self.answer_durable(inbox).await?;
        let state = self.metadata.state();
        let versions: Vec<(Tenant, u64)> = state
            .pending_versions()
            .into_iter()
            .take(Close::CATALOG_MAX)
            .collect();
        let catalog: Vec<CatalogVersion> = versions
            .iter()
            .map(|(tenant, version)| CatalogVersion::new(*tenant, *version))
            .collect();
        let placement = state.pending_placement();
        let evictions: Vec<u128> = state
            .pending_evictions()
            .into_iter()
            .take(Close::EVICTIONS_MAX)
            .collect();
        let closed = self
            .sequencer
            .tick(now, &catalog, placement, &evictions)
            .map_err(|error| stopped(&error))?;
        if closed.is_none() {
            return Ok(());
        }
        let answering = SyncReads {
            reads: self.metadata.reads(),
            in_flight: &mut self.in_flight,
            closed: self.closed,
        };
        let _durable = while_syncing(self.sequencer.sync(), inbox, answering, &mut self.waiting)
            .await
            .map_err(|error| stopped(&error))?;
        self.closed = self.sequencer.last_closed().get();
        if self.learn_missed()? {
            self.sync_metadata(inbox).await?;
        }
        self.answer_closes();
        self.answer_versions();
        self.closes_since_snapshot += 1;
        self.store_snapshots().await
    }

    /// Syncs the metadata group, when this turn appended what an answer waits for, and hands those answers out.
    /// A future dropped at the sync leaves the answers waiting, handed out after the next sync.
    async fn answer_durable(
        &mut self,
        inbox: &mut UnboundedReceiver<Asked>,
    ) -> Result<(), RoleError> {
        if self.durable.is_empty() {
            return Ok(());
        }
        self.sync_metadata(inbox).await?;
        for answer in self.durable.drain(..) {
            // An asker gone has no one to tell; what it asked for is durable all the same.
            match answer {
                Durable::Lease(start, reply) => drop(reply.send(Ok(start))),
                Durable::Registered(reply) => drop(reply.send(Ok(()))),
                Durable::Recorded(found, reply) => drop(reply.send(Ok(found))),
                Durable::Restored(Some((reply, refusal))) => drop(reply.send(Err(refusal))),
                Durable::Restored(None) => {}
            }
        }
        Ok(())
    }

    /// Syncs the metadata group while answering what `inbox` brings on a commit's path, from the state before the sync
    /// (design D17), and applies what it made durable.
    /// A future dropped at the sync leaves the staged commands staged, applied by the next sync.
    async fn sync_metadata(
        &mut self,
        inbox: &mut UnboundedReceiver<Asked>,
    ) -> Result<(), RoleError> {
        let (sync, reads) = self.metadata.syncing();
        let answering = SyncReads {
            reads,
            in_flight: &mut self.in_flight,
            closed: self.closed,
        };
        let durable = while_syncing(sync, inbox, answering, &mut self.waiting)
            .await
            .map_err(|error| stopped(&error))?;
        self.metadata
            .synced(durable)
            .map_err(|error| stopped(&error))
    }

    /// Appends every durable close the group has not learned, oldest first: whether it appended one, which the caller
    /// syncs. A start learns again any the group lost before its sync.
    pub(super) fn learn_missed(&mut self) -> Result<bool, RoleError> {
        let mut learned = false;
        let mut after = self.metadata.state().last_learned();
        while let Some(close) = self
            .sequencer
            .close_after(after)
            .map_err(|error| stopped(&error))?
        {
            let versions: Vec<(Tenant, u64)> = close
                .catalog()
                .iter()
                .map(|version| (version.tenant(), version.version()))
                .collect();
            let _position = self
                .metadata
                .learn_close(
                    close.epoch(),
                    &versions,
                    close.placement(),
                    close.evictions(),
                )
                .map_err(|error| stopped(&error))?;
            after = close.epoch();
            learned = true;
        }
        Ok(learned)
    }

    /// Sends each waiting subscriber its next close, when it is durable now.
    fn answer_closes(&mut self) {
        let sequencer = &self.sequencer;
        let waited = std::mem::take(&mut self.closes_waited);
        self.closes_waited = waited
            .into_iter()
            .filter_map(|(after, reply)| match sequencer.close_after(after) {
                // A subscriber gone has no one to tell, and asks again if it returns.
                Ok(Some(close)) => {
                    drop(reply.send(Ok(close)));
                    None
                }
                Ok(None) => Some((after, reply)),
                Err(error) => {
                    // A subscriber gone has no one to tell.
                    drop(reply.send(Err(ErrorReply::new(ErrorCode::Protocol, error.to_string()))));
                    None
                }
            })
            .collect();
    }

    /// The snapshots due, each durable before its log is trimmed through it.
    /// A future dropped at a trim leaves the snapshot stored and its log trimmed in part or not at all, which recovery
    /// reads past.
    async fn store_snapshots(&mut self) -> Result<(), RoleError> {
        let Some(directory) = self.snapshots.clone() else {
            return Ok(());
        };
        if self.metadata.snapshot_due() {
            let snapshot = self.metadata.snapshot();
            let mut bytes = Vec::new();
            snapshot.encode(&mut bytes);
            snapshot_file::store(&directory.join(METADATA_SNAPSHOT), &bytes)?;
            self.metadata
                .trim(&snapshot)
                .await
                .map_err(|error| stopped(&error))?;
        }
        if self.closes_since_snapshot >= CLOSES_PER_SNAPSHOT {
            let snapshot = self.sequencer.snapshot();
            let mut bytes = Vec::new();
            snapshot.encode(&mut bytes);
            snapshot_file::store(&directory.join(SEQUENCER_SNAPSHOT), &bytes)?;
            self.sequencer
                .trim(&snapshot)
                .await
                .map_err(|error| stopped(&error))?;
            self.closes_since_snapshot = 0;
        }
        Ok(())
    }
}
