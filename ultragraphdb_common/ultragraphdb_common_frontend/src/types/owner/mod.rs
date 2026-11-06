/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod owner_answer;
pub(crate) mod owner_catalog;
mod owner_turn;

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_metadata::{MetadataGroup, MetadataSnapshot};
use ultragraphdb_cluster_placement::NodeId;
use ultragraphdb_cluster_sequencer::{ClockReading, Close, Sequencer, SequencerSnapshot};
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::{DEFAULT_ITERATIONS, ScramVerifier};
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
    /// A default entry's record, and whether it was held already.
    Recorded(bool, Sender<Result<bool, ErrorReply>>),
}

/// A declaring Commit waiting on a catalog version's effective epoch: its tenant, the version and where to answer.
type VersionWaiter = (Tenant, u64, Sender<Result<u64, ErrorReply>>);

/// The one owner of the metadata group, the sequencer and who joined: everything an endpoint core asks is decided here,
/// generic over the log so replication replaces it in stage 8.
pub struct Owner<M: OrderedLog, S: OrderedLog> {
    metadata: MetadataGroup<M>,
    sequencer: Sequencer<S>,
    membership: Membership,
    durable: Vec<Durable>,
    closes_waited: Vec<(Epoch, Sender<Result<Close, ErrorReply>>)>,
    /// The catalog versions a declaring Commit waits on the effective epoch of.
    versions_waited: Vec<VersionWaiter>,
    /// The sessions with a commit in flight.
    in_flight: BTreeSet<u128>,
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
        first_start(&mut metadata, start).await?;
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
            versions_waited: Vec::new(),
            in_flight: BTreeSet::new(),
            closes_since_snapshot: 0,
            snapshots,
        };
        owner.learn_missed().await?;
        Ok(owner)
    }

    /// Creates `tenant` when the group does not hold it, durable before this returns.
    ///
    /// # Errors
    ///
    /// [`RoleError::software`] naming the refusal, or [`RoleError::io`] for a failed log.
    pub async fn create_tenant_once(&mut self, tenant: u64) -> Result<(), RoleError> {
        if self
            .metadata
            .create_tenant(Tenant::new(tenant))
            .map_err(|error| stopped(&error))?
            .is_some()
        {
            let _durable = self
                .metadata
                .sync()
                .await
                .map_err(|error| stopped(&error))?;
        }
        Ok(())
    }

    /// The secret an unknown name's verifier derives from, which the first start drew.
    #[must_use]
    pub fn unknown_secret(&self) -> Option<[u8; 32]> {
        self.metadata.state().unknown_secret().copied()
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

/// The cluster's first start: tenant 0, created by a command of the group, the secret an unknown name's verifier derives from, and
/// the bootstrap administrator from the secret file the settings name, each durable before any role serves.
async fn first_start<L: OrderedLog>(
    metadata: &mut MetadataGroup<L>,
    start: &OwnerStart,
) -> Result<(), RoleError> {
    let tenant = metadata
        .create_tenant(Tenant::new(FIRST_TENANT))
        .map_err(|error| stopped(&error))?;
    let mut secret = [0; 32];
    getrandom::fill(&mut secret)
        .map_err(|error| RoleError::unavailable(format!("the random source: {error}")))?;
    let drawn = metadata
        .draw_secret(secret)
        .map_err(|error| stopped(&error))?;
    let bootstrapped = bootstrap(metadata, start)?;
    if tenant.is_some() || drawn.is_some() || bootstrapped {
        let _durable = metadata.sync().await.map_err(|error| stopped(&error))?;
    }
    Ok(())
}

/// The bootstrap administrator of a group holding no principal, when the settings name its secret file: whether it was created. The
/// file is read once, here, and the group keeps only the verifier derived from it (`SECURITY.md`).
fn bootstrap<L: OrderedLog>(
    metadata: &mut MetadataGroup<L>,
    start: &OwnerStart,
) -> Result<bool, RoleError> {
    let Some(file) = start.bootstrap_file() else {
        return Ok(false);
    };
    if metadata.state().principals() > 0 {
        return Ok(false);
    }
    let held = std::fs::read(file).map_err(|error| {
        RoleError::configuration(format!("metadata.bootstrap_secret_file {file}: {error}"))
    })?;
    let mut salt = [0; 16];
    getrandom::fill(&mut salt)
        .map_err(|error| RoleError::unavailable(format!("the random source: {error}")))?;
    let _created = metadata
        .bootstrap(BOOTSTRAP_NAME, &held, &salt, DEFAULT_ITERATIONS)
        .map_err(|error| RoleError::configuration(error.to_string()))?;
    Ok(true)
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

/// The tenant the cluster's first start creates.
const FIRST_TENANT: u64 = 0;

/// The bootstrap administrator's name.
const BOOTSTRAP_NAME: &str = "admin";

const METADATA_SNAPSHOT: &str = "metadata.snapshot";
const SEQUENCER_SNAPSHOT: &str = "sequencer.snapshot";
