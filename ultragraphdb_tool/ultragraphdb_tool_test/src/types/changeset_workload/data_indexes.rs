/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;

/// The indexes the statements of the default graph produce, by the list each names.
#[derive(Default)]
pub(super) struct DataIndexes {
    /// Each declared tokenizer's token of each literal, to its subjects.
    pub(super) tokens: BTreeMap<(SystemId, u8, Vec<u8>), BTreeSet<SystemId>>,
    pub(super) reverse: BTreeMap<(SystemId, EncodedTerm), BTreeSet<SystemId>>,
    pub(super) subject_predicates: BTreeMap<SystemId, BTreeSet<SystemId>>,
    pub(super) object_predicates: BTreeMap<EncodedTerm, BTreeSet<SystemId>>,
    pub(super) forward_counts: BTreeMap<(SystemId, u32), BTreeSet<SystemId>>,
}
