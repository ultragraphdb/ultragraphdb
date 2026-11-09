/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_codec::PostingType;
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_store_posting::{CachedList, SET, Writes};
use ultragraphdb_store_quads::{QuadStore, WriteTerms};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;

/// The home's own groups `writes` references entities from, as its round adds them (partitions, "The home adds its own
/// group without a registration"): for each edge a forward delta of `tenant` sets, its subject's bucket group with
/// its object, an IRI or blank node of a bucket `quads` owns, unless the object's referrer set names the group already:
/// the set the rounds left in memory, or else the set at `read_ts`. Referrer sets only grow, so a group named at the
/// read, or by a later round, is named at the commit; one the read lacked is checked again when the round applies. An
/// object `terms` created has no referrer set to read. In `(entity, group)` order, each once; derived from the writes
/// alone, so a replay derives the same.
///
/// # Errors
///
/// The store's error; the corruption of a referrer set.
pub(crate) fn referrer_candidates<S, C, B>(
    quads: &QuadStore<S, C, B>,
    tenant: u64,
    read_ts: u64,
    writes: &Writes,
    terms: &WriteTerms,
) -> Result<Vec<(SystemId, BucketGroup)>, ChangesetError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    let mut seen: BTreeSet<(SystemId, BucketGroup)> = BTreeSet::new();
    let mut candidates = Vec::new();
    for (key, delta) in writes.deltas() {
        // The keyspace byte first: most deltas are no forward list's, and parsing them costs.
        if key.first() != Some(&Keyspace::Forward.byte()) {
            continue;
        }
        let Ok((QuadKey::Forward { subject, .. }, None)) = QuadKey::parse(key) else {
            continue;
        };
        let group = BucketGroup::of(subject.bucket());
        for posting in delta.postings() {
            if posting.op() != SET || posting.posting_type() != PostingType::Ref {
                continue;
            }
            let Ok(object) = SystemId::from_u128(posting.uid()) else {
                continue;
            };
            if !quads.owned().owns(object.bucket()) || !seen.insert((object, group)) {
                continue;
            }
            let named = !terms.is_fresh(object)
                && match quads.held_names(tenant, object, group) {
                    Some(named) => named,
                    None => quads
                        .referrers(tenant, object, read_ts)?
                        .member(group)
                        .is_some(),
                };
            if !named {
                candidates.push((object, group));
            }
        }
    }
    candidates.sort_unstable();
    Ok(candidates)
}
