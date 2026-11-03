/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod metadata_state_access;
mod metadata_state_apply;
mod metadata_state_catalog;
mod metadata_state_catalog_source;
mod metadata_state_credentials;

use std::collections::{BTreeMap, VecDeque};

use ultragraphdb_cluster_placement::{PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Epoch, Namespace};

use crate::types::catalog_record::CatalogRecord;
use crate::types::principal::Principal;

/// The metadata group's state: a pure function of the commands applied to it in log order, with no I/O, so equal
/// commands give equal states. [`MetadataState::apply`] is the step function; the queries read it. Control plane:
/// every table is bounded, by the group's settings or by the limits below.
///
/// - **Catalog versions.** Per namespace, the latest version's number and the versions retained, in order, each a
///   [`CatalogRecord`]; a version is pending until a learned close assigns it the epoch after that close. The versions
///   superseded before every partition's untrimmed log begins are released. Beside them, the default-entry records by
///   (namespace, predicate).
/// - **Sequence blocks.** The next sequence of every type that has leased a block; a type absent starts at 1.
/// - **Placement.** The placement map, once initialised, and the epoch each partition's untrimmed log begins at.
/// - **Principals.** By name, each with its identity, and the identity the next one takes.
/// - **Sessions.** In registration order with their principals, indexed by identity, and the evictions no learned close
///   has carried yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataState {
    pub(crate) catalogs: BTreeMap<Namespace, NamespaceCatalog>,
    pub(crate) defaults: BTreeMap<(Namespace, String), Vec<u8>>,
    pub(crate) high_water: BTreeMap<u16, u64>,
    pub(crate) placement: Option<PlacementMap>,
    pub(crate) untrimmed: BTreeMap<PartitionId, Epoch>,
    pub(crate) principals: BTreeMap<String, Principal>,
    pub(crate) next_principal: u64,
    pub(crate) sessions: VecDeque<(u128, u64)>,
    /// The sessions of `sessions` by identity, with their principals: the owner check's index.
    pub(crate) owners: BTreeMap<u128, u64>,
    pub(crate) evictions: VecDeque<u128>,
    pub(crate) learned: Epoch,
}

/// One namespace's catalog versions: the latest version's number, the highest released, and the versions retained in
/// order, the last being the latest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NamespaceCatalog {
    pub(crate) current: u64,
    pub(crate) released: u64,
    pub(crate) versions: VecDeque<CatalogRecord>,
}

impl MetadataState {
    /// The most catalog versions pending at once across every namespace, the most one close announces, 256.
    pub const PENDING_MAX: usize = 256;

    /// The most catalog versions one namespace retains, 1,024.
    pub const RETAINED_MAX: usize = 1_024;

    /// The most default-entry records held, 1,048,576.
    pub const DEFAULTS_MAX: usize = 1 << 20;

    /// The most principals held, 65,536.
    pub const PRINCIPALS_MAX: usize = 1 << 16;

    /// The most session evictions awaiting a close, 65,536.
    pub const EVICTIONS_PENDING_MAX: usize = 1 << 16;

    /// The empty state of a group that has applied no command.
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalogs: BTreeMap::new(),
            defaults: BTreeMap::new(),
            high_water: BTreeMap::new(),
            placement: None,
            untrimmed: BTreeMap::new(),
            principals: BTreeMap::new(),
            next_principal: 1,
            sessions: VecDeque::new(),
            owners: BTreeMap::new(),
            evictions: VecDeque::new(),
            learned: Epoch::ZERO,
        }
    }

    /// The last close the state has learned, epoch 0 before the first.
    #[must_use]
    pub fn last_learned(&self) -> Epoch {
        self.learned
    }
}

impl Default for MetadataState {
    fn default() -> Self {
        Self::new()
    }
}
