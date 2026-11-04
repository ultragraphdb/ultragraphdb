/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod owner_answer;
mod owner_turn;

use std::fmt;
use std::path::PathBuf;

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_metadata::{MetadataGroup, MetadataSnapshot};
use ultragraphdb_cluster_placement::NodeId;
use ultragraphdb_cluster_sequencer::{ClockReading, Close, Sequencer, SequencerSnapshot};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::ErrorReply;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::membership::Membership;
use crate::types::owner_start::OwnerStart;
use crate::utils::snapshot_file;

/// The replica leading every partition while each has one.
const LEADER: u32 = 1;

/// The closes between two of the sequencer's snapshots, which let its log be trimmed.
const CLOSES_PER_SNAPSHOT: u32 = 4_096;

/// An answer that waits for the metadata group's next sync.
#[derive(Debug)]
enum Durable {
    Lease(u64, Sender<Result<u64, ErrorReply>>),
    Registered(Sender<Result<(), ErrorReply>>),
}

/// The one owner of the metadata group, the sequencer and who joined: everything an endpoint core asks is decided here,
/// generic over the log so replication replaces it in stage 8.
pub struct Owner<M: OrderedLog, S: OrderedLog> {
    metadata: MetadataGroup<M>,
    sequencer: Sequencer<S>,
    membership: Membership,
    durable: Vec<Durable>,
    closes_waited: Vec<(Epoch, Sender<Result<Close, ErrorReply>>)>,
    closes_since_snapshot: u32,
    snapshots: Option<PathBuf>,
}

impl<M: OrderedLog, S: OrderedLog> fmt::Debug for Owner<M, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Owner")
            .field("membership", &self.membership)
            .field("closes_waited", &self.closes_waited.len())
            .finish_non_exhaustive()
    }
}

impl<M: OrderedLog, S: OrderedLog> Owner<M, S> {
    /// The owner over its two logs at `now`, recovered from the snapshots in `snapshots` when it names a directory, and
    /// the logs' rounds after them. A metadata group holding no placement forms an even map over the configured
    /// partitions, durable before this returns; one holding a map keeps it. A close the sequencer's log holds and the
    /// group never learned, since the process stopped between the two syncs, is learned here.
    ///
    /// # Errors
    ///
    /// [`RoleError::io`] for a failed log or snapshot file; [`RoleError::software`] for a log or a snapshot that does
    /// not replay; [`RoleError::configuration`] for a partition count the placement cannot hold.
    pub async fn recover(
        metadata_log: M,
        sequencer_log: S,
        start: &OwnerStart,
        snapshots: Option<PathBuf>,
        now: ClockReading,
    ) -> Result<Self, RoleError> {
        let (metadata_snapshot, sequencer_snapshot) = match &snapshots {
            Some(directory) => (
                snapshot_file::load(&directory.join(METADATA_SNAPSHOT))?
                    .map(|bytes| MetadataSnapshot::decode(&bytes))
                    .transpose()
                    .map_err(|error| stopped(&error))?,
                snapshot_file::load(&directory.join(SEQUENCER_SNAPSHOT))?
                    .map(|bytes| SequencerSnapshot::decode(&bytes))
                    .transpose()
                    .map_err(|error| stopped(&error))?,
            ),
            None => (None, None),
        };
        let mut metadata =
            MetadataGroup::recover(metadata_log, *start.metadata(), metadata_snapshot)
                .await
                .map_err(|error| stopped(&error))?;
        let sequencer =
            Sequencer::recover_from(sequencer_log, *start.sequencer(), now, sequencer_snapshot)
                .await
                .map_err(|error| stopped(&error))?;
        if metadata.state().placement().is_none() {
            let _position = metadata
                .init_placement(start.partition_count(), NodeId::new(LEADER))
                .map_err(|error| RoleError::configuration(error.to_string()))?;
            let _durable = metadata.sync().await.map_err(|error| stopped(&error))?;
        }
        let partitions = placed_partitions(&metadata).unwrap_or(start.partition_count());
        if partitions != start.partition_count() {
            tracing::warn!(
                held = partitions,
                configured = start.partition_count(),
                "the metadata group holds a placement over another partition count, which it keeps"
            );
        }
        let mut owner = Self {
            metadata,
            sequencer,
            membership: Membership::new(partitions),
            durable: Vec::new(),
            closes_waited: Vec::new(),
            closes_since_snapshot: 0,
            snapshots,
        };
        owner.learn_missed().await?;
        Ok(owner)
    }

    /// Creates the principal `name` holding `verifier` when the group does not hold it, durable before this returns.
    ///
    /// # Errors
    ///
    /// [`RoleError::software`] naming the refusal, or [`RoleError::io`] for a failed log.
    pub async fn create_principal_once(
        &mut self,
        name: &str,
        verifier: ScramVerifier,
    ) -> Result<(), RoleError> {
        if self.metadata.state().principal(name).is_some() {
            return Ok(());
        }
        let _created = self
            .metadata
            .create_principal(name, verifier, false)
            .map_err(|error| stopped(&error))?;
        let _durable = self
            .metadata
            .sync()
            .await
            .map_err(|error| stopped(&error))?;
        Ok(())
    }
}

/// What an owner's failure stops the process with: a failed log's code, or a bug's.
pub(crate) fn stopped(error: &ClusterError) -> RoleError {
    if matches!(error.0, ClusterErrorEnum::Log(_)) {
        RoleError::io(error.to_string())
    } else {
        RoleError::software(error.to_string())
    }
}

/// The highest partition of the placement the group holds.
fn placed_partitions<L: OrderedLog>(metadata: &MetadataGroup<L>) -> Option<u32> {
    let map = metadata.state().placement()?;
    map.ranges_at(map.latest_change())
        .iter()
        .map(|(_, partition)| partition.get())
        .max()
}

const METADATA_SNAPSHOT: &str = "metadata.snapshot";
const SEQUENCER_SNAPSHOT: &str = "sequencer.snapshot";
