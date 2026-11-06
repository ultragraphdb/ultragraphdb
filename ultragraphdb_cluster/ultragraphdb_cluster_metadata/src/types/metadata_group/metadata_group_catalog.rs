/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_traits_log::{OrderedLog, Position};

use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_group::MetadataGroup;

impl<L: OrderedLog> MetadataGroup<L> {
    /// Commits `bytes` as a new version of `tenant`'s catalog built on version `predecessor`, pending until a close
    /// announces it; returns its number, `predecessor + 1`, and the position it was appended at.
    ///
    /// # Errors
    ///
    /// [`ClusterError::stale_catalog`] naming `predecessor` and the latest version when `predecessor` is not the
    /// latest; the other refusals of [`MetadataState::apply`](crate::MetadataState::apply) for
    /// [`MetadataCommand::OfferCatalog`](crate::MetadataCommand::OfferCatalog); [`ClusterError::log`] when the log
    /// refuses the round. Nothing is appended on a refusal.
    pub fn offer_catalog(
        &mut self,
        tenant: Tenant,
        predecessor: u64,
        bytes: &[u8],
    ) -> Result<(u64, Position), ClusterError> {
        let position = self.append(MetadataCommand::OfferCatalog {
            tenant,
            predecessor,
            bytes: bytes.to_vec(),
        })?;
        // The offer was taken, so its predecessor is below the largest version.
        Ok((predecessor + 1, position))
    }

    /// Creates `tenant`; returns the position it was appended at, or `None` for a tenant the group holds, which changes nothing,
    /// so a restart creates none.
    ///
    /// # Errors
    ///
    /// The refusals of [`MetadataState::apply`](crate::MetadataState::apply) for
    /// [`MetadataCommand::CreateTenant`](crate::MetadataCommand::CreateTenant); [`ClusterError::log`].
    pub fn create_tenant(&mut self, tenant: Tenant) -> Result<Option<Position>, ClusterError> {
        if self.head.holds_tenant(tenant) {
            return Ok(None);
        }
        self.append(MetadataCommand::CreateTenant { tenant })
            .map(Some)
    }

    /// Holds `secret`, what an unknown name's verifier derives from; returns the position it was appended at, or `None` once a
    /// secret was drawn, which is kept, so a restart answers one name with one salt.
    ///
    /// # Errors
    ///
    /// [`ClusterError::log`] when the log refuses the round.
    pub fn draw_secret(&mut self, secret: [u8; 32]) -> Result<Option<Position>, ClusterError> {
        if self.head.unknown_secret().is_some() {
            return Ok(None);
        }
        self.append(MetadataCommand::DrawSecret { secret })
            .map(Some)
    }

    /// Records `entry`, the default entry a first write of an undeclared predicate asks for, keyed by its tenant and
    /// predicate; returns the position it was appended at, or `None` when a record of that key exists, which is kept
    /// and nothing is appended. It creates no catalog version, so it stales no prepared statement.
    ///
    /// # Errors
    ///
    /// The refusals of [`MetadataState::apply`](crate::MetadataState::apply) for
    /// [`MetadataCommand::RecordDefault`](crate::MetadataCommand::RecordDefault); [`ClusterError::log`].
    pub fn record_default_entry(
        &mut self,
        entry: &DefaultEntry,
    ) -> Result<Option<Position>, ClusterError> {
        let tenant = Tenant::new(entry.tenant());
        if self.head.default_entry(tenant, entry.predicate()).is_some() {
            return Ok(None);
        }
        let position = self.append(MetadataCommand::RecordDefault {
            tenant,
            predicate: entry.predicate().to_string(),
            entry: entry.entry().to_vec(),
        })?;
        Ok(Some(position))
    }

    /// Learns the sequencer's durable close of `epoch`, which announced `versions` and `placement` (0 for none), each
    /// taking effect at the epoch after it, and carried `evictions`, which leave the evictions pending; returns the position it was appended at,
    /// or `None` for a close at or before the last learned, which changes nothing, so the host may hand over every
    /// close the sequencer holds after a restart.
    ///
    /// # Errors
    ///
    /// The refusals of [`MetadataState::apply`](crate::MetadataState::apply) for
    /// [`MetadataCommand::LearnClose`](crate::MetadataCommand::LearnClose); [`ClusterError::too_many`] for more than
    /// [`MetadataCommand::VERSIONS_MAX`](crate::MetadataCommand::VERSIONS_MAX) versions or
    /// [`MetadataCommand::EVICTIONS_MAX`](crate::MetadataCommand::EVICTIONS_MAX) evictions; [`ClusterError::log`].
    pub fn learn_close(
        &mut self,
        epoch: Epoch,
        versions: &[(Tenant, u64)],
        placement: u64,
        evictions: &[u128],
    ) -> Result<Option<Position>, ClusterError> {
        if epoch <= self.head.last_learned() {
            return Ok(None);
        }
        let position = self.append(MetadataCommand::LearnClose {
            epoch,
            versions: versions.to_vec(),
            placement,
            evictions: evictions.to_vec(),
        })?;
        Ok(Some(position))
    }

    /// Records that `partition`'s untrimmed log begins at `epoch`, so the catalog versions superseded at or before the
    /// oldest such epoch of every partition the placement map's latest layout names are released; returns the position
    /// it was appended at.
    ///
    /// # Errors
    ///
    /// [`ClusterError::log`] when the log refuses the round.
    pub fn report_untrimmed(
        &mut self,
        partition: PartitionId,
        epoch: Epoch,
    ) -> Result<Position, ClusterError> {
        self.append(MetadataCommand::ReportUntrimmed { partition, epoch })
    }
}
