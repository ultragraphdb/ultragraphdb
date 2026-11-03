/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix};
use ultragraphdb_error_cluster::ClusterError;

use crate::types::catalog_record::CatalogRecord;
use crate::types::metadata_state::MetadataState;

impl MetadataState {
    /// The number of `namespace`'s latest version, pending or not, 0 before its first: the predecessor a new version
    /// names.
    #[must_use]
    pub fn current_version(&self, namespace: Namespace) -> u64 {
        self.catalogs
            .get(&namespace)
            .map_or(0, |catalog| catalog.current)
    }

    /// The versions pending, by namespace and number, in namespace and version order: those no learned close has
    /// announced, which the sequencer's next close announces. At most [`MetadataState::PENDING_MAX`].
    #[must_use]
    pub fn pending_versions(&self) -> Vec<(Namespace, u64)> {
        self.catalogs
            .iter()
            .flat_map(|(namespace, catalog)| {
                catalog
                    .versions
                    .iter()
                    .filter(|v| v.effective().is_none())
                    .map(|v| (*namespace, v.version()))
            })
            .collect()
    }

    /// The version of `namespace`'s catalog in effect at `epoch`: the latest announced to take effect at or before it,
    /// `None` when none is, the namespace's catalog then being empty. Every epoch up to one after the last learned
    /// close is answered, since a version pending now takes effect no earlier than two epochs after it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::epoch_ahead`] naming `epoch` and the furthest epoch answered when `epoch` lies further;
    /// [`ClusterError::catalog_released`] when the version in effect at `epoch` is released.
    pub fn catalog_at(
        &self,
        namespace: Namespace,
        epoch: Epoch,
    ) -> Result<Option<&CatalogRecord>, ClusterError> {
        // The last learned close is below 2^42 − 1, so one more cannot overflow.
        let furthest = self.learned.get() + 1;
        if epoch.get() > furthest {
            return Err(ClusterError::epoch_ahead(epoch.get(), furthest));
        }
        let Some(catalog) = self.catalogs.get(&namespace) else {
            return Ok(None);
        };
        let in_effect = catalog
            .versions
            .iter()
            .rev()
            .find(|v| v.effective().is_some_and(|e| e <= epoch));
        if in_effect.is_none() && catalog.released > 0 {
            return Err(ClusterError::catalog_released(namespace.get(), epoch.get()));
        }
        Ok(in_effect)
    }

    /// The versions of `namespace`'s catalog retained, in order, pending ones last: those a partition loads before it
    /// replays its log.
    #[must_use]
    pub fn versions(&self, namespace: Namespace) -> Vec<&CatalogRecord> {
        self.catalogs
            .get(&namespace)
            .map(|catalog| catalog.versions.iter().collect())
            .unwrap_or_default()
    }

    /// The namespaces holding a catalog version, in order.
    #[must_use]
    pub fn namespaces(&self) -> Vec<Namespace> {
        self.catalogs.keys().copied().collect()
    }

    /// The default entry recorded for `predicate` in `namespace`, `None` when none is.
    #[must_use]
    pub fn default_entry(&self, namespace: Namespace, predicate: &str) -> Option<&[u8]> {
        self.defaults
            .get(&(namespace, predicate.to_string()))
            .map(Vec::as_slice)
    }

    /// The first sequence of `prefix`'s next block: the type's high-water mark, 1 before its first lease.
    #[must_use]
    pub fn next_sequence(&self, prefix: TypePrefix) -> u64 {
        self.high_water.get(&prefix.get()).copied().unwrap_or(1)
    }

    /// The placement map, `None` before it is initialised.
    #[must_use]
    pub fn placement(&self) -> Option<&PlacementMap> {
        self.placement.as_ref()
    }

    /// The oldest epoch `partition`'s untrimmed log may name, as it last reported, `None` before its first report.
    #[must_use]
    pub fn untrimmed(&self, partition: PartitionId) -> Option<Epoch> {
        self.untrimmed.get(&partition).copied()
    }
}
