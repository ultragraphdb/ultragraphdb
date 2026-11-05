/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The referrer sets a round derives once its versions are known (partitions, "An entity's referrer set names where its
//! inbound edges may be"): each member a registration sets, and each of the home's own groups its edges add, with the
//! commit version's epoch, in round order, so two writes of one set in one round both land and a replay of the round
//! derives the same.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry as MapEntry;
use std::num::NonZeroU64;

use ultragraphdb_core_ids::{SystemId, Version};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::ReferrerSet;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::executed::Executed;

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// Adds to each execution of `round` the referrer sets its round writes, at its commit version: a registration's
    /// member set as [`ReferrerSet::set`] sets it, and each of the home's own groups the set still lacks added at
    /// generation 1, add-if-absent, so concurrent commits never conflict on a set. Each set is read once, at the
    /// watermark under the apply lock the caller holds, and changed in round order on top.
    ///
    /// # Errors
    ///
    /// The store's error; the corruption of a referrer set.
    pub(crate) fn derive_referrers(
        &self,
        round: &mut [(u64, Executed)],
    ) -> Result<(), ChangesetError> {
        let read_ts = self.quads.lists().applied_ts();
        let mut sets: BTreeMap<(u64, SystemId), ReferrerSet> = BTreeMap::new();
        let mut changed: Vec<SystemId> = Vec::new();
        for (version, executed) in round.iter_mut() {
            if executed.referrers.is_empty() && executed.registration.is_none() {
                continue;
            }
            let epoch = Version::from_u64(*version).epoch();
            let tenant = executed.tenant;
            changed.clear();
            for &(entity, group) in &executed.referrers {
                let set = self.held_set(&mut sets, tenant, entity, read_ts)?;
                if set.member(group).is_none() {
                    set.set(group, NonZeroU64::MIN, epoch);
                    if !changed.contains(&entity) {
                        changed.push(entity);
                    }
                }
            }
            if let Some((entity, group, generation)) = executed.registration {
                self.held_set(&mut sets, tenant, entity, read_ts)?
                    .set(group, generation, epoch);
                if !changed.contains(&entity) {
                    changed.push(entity);
                }
            }
            for &entity in &changed {
                let Some(set) = sets.get(&(tenant, entity)) else {
                    continue;
                };
                let mut value = Vec::new();
                set.encode(&mut value);
                let key = QuadKey::Referrers { tenant, entity }.to_bytes();
                executed.writes.add_entry(Entry::put(key, 0, 0, value));
            }
        }
        Ok(())
    }

    /// The referrer set of `entity` in `tenant` as the round has left it so far: read at `read_ts` the first time.
    fn held_set<'s>(
        &self,
        sets: &'s mut BTreeMap<(u64, SystemId), ReferrerSet>,
        tenant: u64,
        entity: SystemId,
        read_ts: u64,
    ) -> Result<&'s mut ReferrerSet, ChangesetError> {
        Ok(match sets.entry((tenant, entity)) {
            MapEntry::Occupied(held) => held.into_mut(),
            MapEntry::Vacant(vacant) => {
                vacant.insert(self.quads.referrers(tenant, entity, read_ts)?)
            }
        })
    }
}
