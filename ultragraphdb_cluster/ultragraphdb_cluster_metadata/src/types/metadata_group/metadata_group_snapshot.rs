/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::metadata_group::MetadataGroup;
use crate::types::metadata_snapshot::MetadataSnapshot;

impl<L: OrderedLog> MetadataGroup<L> {
    /// The durable state as of [`applied`](Self::applied), as a snapshot for the host to store.
    #[must_use]
    pub fn snapshot(&self) -> MetadataSnapshot {
        MetadataSnapshot::new(self.applied, self.state.clone())
    }

    /// Whether `snapshot_rounds` rounds or more have become durable since the last snapshot trimmed the log, or since
    /// the recovery's snapshot.
    #[must_use]
    pub fn snapshot_due(&self) -> bool {
        let since = self.applied.get().saturating_sub(self.snapshot_at.get());
        since >= u64::from(self.config.snapshot_rounds())
    }

    /// Trims the log through `snapshot`'s position, which the host has stored durably, so the next recovery starts from
    /// it; a snapshot at or below the log's checkpoint changes nothing.
    ///
    /// # Errors
    ///
    /// [`ClusterError::log`] when the log refuses the trim: a position past its committed one, or a failure, which
    /// poisons it.
    pub async fn trim(&mut self, snapshot: &MetadataSnapshot) -> Result<(), ClusterError> {
        self.log.trim_through(snapshot.position()).await?;
        self.snapshot_at = self.snapshot_at.max(snapshot.position());
        Ok(())
    }
}
