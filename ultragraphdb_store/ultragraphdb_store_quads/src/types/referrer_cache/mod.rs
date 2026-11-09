/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;
use std::mem::size_of_val;

use ultragraphdb_core_ids::{BucketGroup, SystemId};

use crate::types::referrer_set::ReferrerSet;

/// The bytes a partition's referrer sets may hold in memory unless its store is opened with another budget.
pub const REFERRER_CACHE_BYTES: usize = 16 << 20;

/// What a held set costs beside its members: its key, its vector and the map's slot.
const ENTRY_BYTES: usize = 64;

/// The referrer sets a partition's rounds left, each as the store holds it at the applied watermark, within a budget of
/// bytes (partitions, "An entity's referrer set names where its inbound edges may be"). Only a round's apply puts a set,
/// once its batch is written, so a held set is the stored one; a drop of a tenant's data forgets the tenant. Two
/// generations, each at most half the budget: once the young one is full it becomes the old one, and a set found in the
/// old one moves back to the young.
#[derive(Debug, Default)]
pub struct ReferrerCache {
    budget: usize,
    young: HashMap<(u64, SystemId), ReferrerSet>,
    young_bytes: usize,
    old: HashMap<(u64, SystemId), ReferrerSet>,
}

impl ReferrerCache {
    /// A cache holding nothing, its sets at most `budget` bytes together.
    #[must_use]
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    /// Whether the held set of `entity` in `tenant` names `group`; `None` when no set is held.
    pub fn names(&mut self, tenant: u64, entity: SystemId, group: BucketGroup) -> Option<bool> {
        self.held(tenant, entity)
            .map(|set| set.member(group).is_some())
    }

    /// The held set of `entity` in `tenant`, if any.
    pub fn get(&mut self, tenant: u64, entity: SystemId) -> Option<ReferrerSet> {
        self.held(tenant, entity).cloned()
    }

    /// Holds `set` as the set of `entity` in `tenant`, in place of any held before; a set larger than half the budget is
    /// not held.
    pub fn put(&mut self, tenant: u64, entity: SystemId, set: ReferrerSet) {
        let key = (tenant, entity);
        let _old: Option<ReferrerSet> = self.old.remove(&key);
        if let Some(before) = self.young.remove(&key) {
            self.young_bytes = self.young_bytes.saturating_sub(cost(&before));
        }
        let bytes = cost(&set);
        let half = self.budget / 2;
        if bytes > half {
            return;
        }
        if self.young_bytes + bytes > half {
            self.old = std::mem::take(&mut self.young);
            self.young_bytes = 0;
        }
        self.young_bytes += bytes;
        let _none: Option<ReferrerSet> = self.young.insert(key, set);
    }

    /// Forgets every set of `tenant`.
    pub fn forget(&mut self, tenant: u64) {
        let mut freed = 0;
        self.young.retain(|&(t, _), set| {
            let keep = t != tenant;
            if !keep {
                freed += cost(set);
            }
            keep
        });
        self.young_bytes = self.young_bytes.saturating_sub(freed);
        self.old.retain(|&(t, _), _| t != tenant);
    }

    /// The sets held, both generations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.young.len() + self.old.len()
    }

    /// Whether no set is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.young.is_empty() && self.old.is_empty()
    }

    /// The held set of `entity` in `tenant`, one found in the old generation moved to the young.
    fn held(&mut self, tenant: u64, entity: SystemId) -> Option<&ReferrerSet> {
        let key = (tenant, entity);
        if !self.young.contains_key(&key) {
            let set = self.old.remove(&key)?;
            self.put(tenant, entity, set);
        }
        self.young.get(&key)
    }
}

/// The bytes `set` costs held.
fn cost(set: &ReferrerSet) -> usize {
    ENTRY_BYTES + size_of_val(set.members())
}
