/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_store_posting::Request;
use ultragraphdb_store_quads::WriteTerms;

use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::rebuild::Rebuild;

/// What a changeset's write leaves to commit: its request and terms, the catalog its commit leaves and the keys of the
/// implicit default entries it writes, the build its
/// declaration starts, the subjects whose vector it changed while a vector index is built, and the index keys a
/// request after it deletes.
pub(crate) struct WriteOutcome<'a, S, C> {
    pub request: Request<'a, S, C>,
    pub terms: WriteTerms,
    pub catalog: Catalog,
    /// The catalog keys written as a predicate's implicit default entry, sorted.
    pub defaults: Vec<Vec<u8>>,
    /// The predicates of `defaults`, in the same order.
    pub default_uids: Vec<u64>,
    /// Whether the write changes the catalog: a declaration, or the namespace's initial catalog.
    pub catalog_changed: bool,
    pub pending: Option<Rebuild>,
    pub touched: BTreeSet<(u64, u64)>,
    pub removals: Vec<PredicatePrefix>,
}
