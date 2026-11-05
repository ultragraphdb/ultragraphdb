/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_mapping::PredicateDeclaration;

use crate::types::index_plan::IndexPlan;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::utils::quad_prefixes::declared_prefixes;

/// A build of a predicate's indexes: the declarations before and after the change that started it, its plan, the
/// change's commit timestamp, at which the build reads, and its record's number. While it runs, writes are applied
/// under [`Rebuild::maintained`] and record the subjects whose vector they change, which the swap applies to a
/// vector index the build made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rebuild {
    pub tenant: u64,
    pub predicate: SystemId,
    pub old: PredicateDeclaration,
    pub new: PredicateDeclaration,
    pub plan: IndexPlan,
    pub start_ts: u64,
    pub record: u64,
    pub touched: BTreeSet<(Graph, SystemId)>,
}

impl Rebuild {
    /// The build a changeset's declaration asks for, before the changeset commits.
    pub fn pending(
        tenant: u64,
        predicate: SystemId,
        old: PredicateDeclaration,
        new: PredicateDeclaration,
    ) -> Self {
        let plan = IndexPlan::of(&old, &new);
        Self {
            tenant,
            predicate,
            old,
            new,
            plan,
            start_ts: 0,
            record: 0,
            touched: BTreeSet::new(),
        }
    }

    /// The declaration writes are applied under while the build runs.
    pub fn maintained(&self) -> PredicateDeclaration {
        self.plan.maintained(&self.old, &self.new)
    }

    /// The declaration queries read, and the catalog holds, while the build runs.
    pub fn interim(&self) -> PredicateDeclaration {
        self.plan.interim(&self.new)
    }

    /// The indexes writes maintain while the build runs, which its record names: the old declaration's and the new
    /// one's.
    pub fn maintained_prefixes(&self) -> Vec<PredicatePrefix> {
        let mut prefixes = declared_prefixes(self.tenant, self.predicate, &self.old);
        for p in declared_prefixes(self.tenant, self.predicate, &self.new) {
            if !prefixes.contains(&p) {
                prefixes.push(p);
            }
        }
        prefixes
    }
}
