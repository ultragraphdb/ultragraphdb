/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_cluster_placement::PartitionId;

/// Where a catalog version stands against the partitions' checks of it (`PROTOCOL.md` §11.13).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum CatalogCheck {
    /// It stands: it asked for no check, or every partition holding its tenant passed it.
    #[default]
    Settled,
    /// It tightens a predicate, and the partitions are checking it.
    Checking {
        /// The partitions that reported their check passed.
        passed: BTreeSet<PartitionId>,
        /// Whether a partition reported the data refuses it.
        refused: bool,
    },
    /// A restoring version withdrew it: it reached no client as declared.
    Withdrawn,
}

impl CatalogCheck {
    /// A check no partition has reported on.
    #[must_use]
    pub fn begun() -> Self {
        Self::Checking {
            passed: BTreeSet::new(),
            refused: false,
        }
    }
}
