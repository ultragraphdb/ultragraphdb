/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion};

use crate::types::catalog_record::CatalogRecord;
use crate::types::metadata_state::MetadataState;

impl CatalogSource for MetadataState {
    /// The group's refusal: an epoch past the furthest it answers, or a version released.
    type Error = ClusterError;

    /// The version in effect at `epoch`, as [`MetadataState::catalog_at`] answers it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::epoch_ahead`] when `epoch` lies past one after the last learned close, or past 42 bits;
    /// [`ClusterError::catalog_released`] when the version in effect at `epoch` is released.
    fn version_at(&self, namespace: u64, epoch: u64) -> Result<Option<HeldVersion>, Self::Error> {
        let at = Epoch::new(epoch)
            .map_err(|_| ClusterError::epoch_ahead(epoch, self.learned.get() + 1))?;
        Ok(self
            .catalog_at(Namespace::new(namespace), at)?
            .and_then(held))
    }

    /// Version `version` of `namespace`, `None` when it is pending, released or was never committed.
    fn version(&self, namespace: u64, version: u64) -> Result<Option<HeldVersion>, Self::Error> {
        Ok(self
            .catalogs
            .get(&Namespace::new(namespace))
            .and_then(|catalog| catalog.versions.iter().find(|v| v.version() == version))
            .and_then(held))
    }
}

/// `record` as a partition holds it, `None` while it is pending.
fn held(record: &CatalogRecord) -> Option<HeldVersion> {
    let effective = record.effective()?;
    Some(HeldVersion::new(
        record.namespace().get(),
        record.version(),
        effective.get(),
        record.bytes().to_vec(),
    ))
}
