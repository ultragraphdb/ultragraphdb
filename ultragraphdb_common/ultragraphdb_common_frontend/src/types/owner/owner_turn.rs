/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use ultragraphdb_cluster_sequencer::{CatalogVersion, ClockReading, Close};
use ultragraphdb_core_ids::Namespace;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};
use ultragraphdb_traits_log::OrderedLog;

use crate::types::asked::Asked;
use crate::types::owner::{
    CLOSES_PER_SNAPSHOT, Durable, METADATA_SNAPSHOT, Owner, SEQUENCER_SNAPSHOT, stopped,
};
use crate::utils::snapshot_file;

impl<M: OrderedLog, S: OrderedLog> Owner<M, S> {
    /// Runs the owner until every [`crate::OwnerHandle`] is gone: each turn takes whatever has been asked, then answers
    /// what its sync made durable and closes an epoch that is due, by `clock`.
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
                while let Ok(next) = inbox.try_recv() {
                    self.take(next)?;
                }
            }
            self.turn(clock()).await?;
        }
    }

    /// One turn: the answers the asks of this turn wait for, then the close due at `now`, durable in the sequencer's log
    /// before the group learns it.
    async fn turn(&mut self, now: ClockReading) -> Result<(), RoleError> {
        if !self.durable.is_empty() {
            let _durable = self
                .metadata
                .sync()
                .await
                .map_err(|error| stopped(&error))?;
            for answer in self.durable.drain(..) {
                // An asker gone has no one to tell; what it asked for is durable all the same.
                match answer {
                    Durable::Lease(start, reply) => drop(reply.send(Ok(start))),
                    Durable::Registered(reply) => drop(reply.send(Ok(()))),
                }
            }
        }
        let state = self.metadata.state();
        let versions: Vec<(Namespace, u64)> = state
            .pending_versions()
            .into_iter()
            .take(Close::CATALOG_MAX)
            .collect();
        let catalog: Vec<CatalogVersion> = versions
            .iter()
            .map(|(namespace, version)| CatalogVersion::new(*namespace, *version))
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
        let _durable = self
            .sequencer
            .sync()
            .await
            .map_err(|error| stopped(&error))?;
        self.learn_missed().await?;
        self.answer_closes();
        self.closes_since_snapshot += 1;
        self.store_snapshots().await
    }

    /// Learns every durable close the group has not, oldest first, durable before it returns.
    pub(super) async fn learn_missed(&mut self) -> Result<(), RoleError> {
        let mut learned = false;
        let mut after = self.metadata.state().last_learned();
        while let Some(close) = self
            .sequencer
            .close_after(after)
            .map_err(|error| stopped(&error))?
        {
            let versions: Vec<(Namespace, u64)> = close
                .catalog()
                .iter()
                .map(|version| (version.namespace(), version.version()))
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
        if learned {
            let _durable = self
                .metadata
                .sync()
                .await
                .map_err(|error| stopped(&error))?;
        }
        Ok(())
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
                    drop(reply.send(Err(ErrorReply::new(ErrorCode::Protocol, error.to_string()))));
                    None
                }
            })
            .collect();
    }

    /// The snapshots due, each durable before its log is trimmed through it.
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
