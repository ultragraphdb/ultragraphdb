/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;

use crate::types::catalog_version::CatalogVersion;

/// The close of an epoch, as the sequencer records it durably and delivers it: the epoch, the sequencer's wall-clock
/// time in microseconds, never below the close before it, the catalog versions it announces, each taking effect at the
/// next epoch, the placement version it announces, at most one, taking effect then too, and the sessions the registry
/// evicted since the close before, by their 128-bit identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Close {
    epoch: Epoch,
    time_us: u64,
    catalog: Vec<CatalogVersion>,
    placement: u64,
    evictions: Vec<u128>,
}

impl Close {
    /// The most catalog versions one close announces, 256.
    pub const CATALOG_MAX: usize = 256;

    /// The most session evictions one close carries, 4,096.
    pub const EVICTIONS_MAX: usize = 4_096;

    /// The close of `epoch` at `time_us`, announcing `catalog` and carrying `evictions`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::too_many`] when `catalog` holds more than [`Close::CATALOG_MAX`] versions or `evictions` more
    /// than [`Close::EVICTIONS_MAX`] sessions.
    pub fn new(
        epoch: Epoch,
        time_us: u64,
        catalog: Vec<CatalogVersion>,
        evictions: Vec<u128>,
    ) -> Result<Self, ClusterError> {
        if catalog.len() > Self::CATALOG_MAX {
            return Err(ClusterError::too_many(
                "catalog versions in a close",
                catalog.len(),
                Self::CATALOG_MAX,
            ));
        }
        if evictions.len() > Self::EVICTIONS_MAX {
            return Err(ClusterError::too_many(
                "session evictions in a close",
                evictions.len(),
                Self::EVICTIONS_MAX,
            ));
        }
        Ok(Self {
            epoch,
            time_us,
            catalog,
            placement: 0,
            evictions,
        })
    }

    /// The close announcing placement version `version`, the metadata group's oldest pending one; 0 announces none.
    #[must_use]
    pub fn with_placement(mut self, version: u64) -> Self {
        self.placement = version;
        self
    }

    /// The epoch closed.
    #[must_use]
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// The sequencer's wall-clock time at the close, in microseconds.
    #[must_use]
    pub fn time_us(&self) -> u64 {
        self.time_us
    }

    /// The catalog versions the close announces.
    #[must_use]
    pub fn catalog(&self) -> &[CatalogVersion] {
        &self.catalog
    }

    /// The placement version the close announces, 0 for none.
    #[must_use]
    pub fn placement(&self) -> u64 {
        self.placement
    }

    /// The sessions evicted since the close before, by their 128-bit identities.
    #[must_use]
    pub fn evictions(&self) -> &[u128] {
        &self.evictions
    }

    /// The epoch the announced catalog and placement versions take effect at, one after the closed epoch; `None` for the close of the
    /// last epoch, 2^42 − 1, which no epoch follows.
    #[must_use]
    pub fn effective_epoch(&self) -> Option<Epoch> {
        self.epoch.next().ok()
    }
}
