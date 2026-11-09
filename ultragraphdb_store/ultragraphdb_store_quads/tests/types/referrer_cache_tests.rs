/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::mem::size_of;
use std::num::NonZeroU64;

use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, Sequence, SystemId, TypePrefix};
use ultragraphdb_store_quads::{REFERRER_CACHE_BYTES, ReferrerCache, ReferrerMember, ReferrerSet};
use ultragraphdb_tool_bdd::TestResult;

/// What a held set costs beside its members, as the cache counts it.
const ENTRY_BYTES: usize = 64;

fn entity(sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(1)?,
        Sequence::new(sequence)?,
    ))
}

/// A set naming `groups`, each at generation 1 and epoch 1.
fn set_of(groups: &[u32]) -> TestResult<ReferrerSet> {
    let mut set = ReferrerSet::new();
    for &group in groups {
        set.set(BucketGroup::new(group)?, NonZeroU64::MIN, Epoch::new(1)?);
    }
    Ok(set)
}

/// The bytes a set of `members` members costs held.
fn cost(members: usize) -> usize {
    ENTRY_BYTES + members * size_of::<ReferrerMember>()
}

#[test]
fn test_a_held_set_answers_whether_it_names_a_group() -> TestResult {
    let mut cache = ReferrerCache::new(REFERRER_CACHE_BYTES);
    let o = entity(1)?;
    cache.put(7, o, set_of(&[3, 5])?);
    assert_eq!(cache.names(7, o, BucketGroup::new(3)?), Some(true));
    assert_eq!(cache.names(7, o, BucketGroup::new(4)?), Some(false));
    assert_eq!(cache.get(7, o), Some(set_of(&[3, 5])?));
    Ok(())
}

#[test]
fn test_a_set_not_held_is_none_in_another_tenant_too() -> TestResult {
    let mut cache = ReferrerCache::new(REFERRER_CACHE_BYTES);
    let o = entity(1)?;
    cache.put(7, o, set_of(&[3])?);
    assert_eq!(cache.names(8, o, BucketGroup::new(3)?), None);
    assert_eq!(cache.names(7, entity(2)?, BucketGroup::new(3)?), None);
    assert!(cache.get(8, o).is_none());
    Ok(())
}

#[test]
fn test_a_put_replaces_the_set_held_before() -> TestResult {
    let mut cache = ReferrerCache::new(REFERRER_CACHE_BYTES);
    let o = entity(1)?;
    cache.put(7, o, set_of(&[3])?);
    cache.put(7, o, set_of(&[3, 9])?);
    assert_eq!(cache.names(7, o, BucketGroup::new(9)?), Some(true));
    assert_eq!(cache.len(), 1);
    assert!(
        !cache.is_empty(),
        "the young generation holds it, the old none"
    );
    Ok(())
}

#[test]
fn test_the_budget_keeps_two_generations_of_half_of_it_each() -> TestResult {
    // A half holds two one-member sets: the third starts a new young generation, the first two become the old one, and
    // the fifth drops them.
    let mut cache = ReferrerCache::new(4 * cost(1));
    for sequence in 1..=5 {
        cache.put(7, entity(sequence)?, set_of(&[1])?);
    }
    assert_eq!(cache.len(), 3);
    assert_eq!(cache.names(7, entity(1)?, BucketGroup::new(1)?), None);
    assert_eq!(cache.names(7, entity(2)?, BucketGroup::new(1)?), None);
    for sequence in 3..=5 {
        assert_eq!(
            cache.names(7, entity(sequence)?, BucketGroup::new(1)?),
            Some(true)
        );
    }
    Ok(())
}

#[test]
fn test_a_set_found_in_the_old_generation_moves_back_to_the_young() -> TestResult {
    let mut cache = ReferrerCache::new(4 * cost(1));
    for sequence in 1..=3 {
        cache.put(7, entity(sequence)?, set_of(&[1])?);
    }
    // Young holds 3; old holds 1 and 2. Reading 1 moves it to the young, which is then full.
    assert_eq!(cache.names(7, entity(1)?, BucketGroup::new(1)?), Some(true));
    // The next put rotates: the young (3 and 1) becomes the old, so 2 is gone and 1 stays.
    cache.put(7, entity(4)?, set_of(&[1])?);
    assert_eq!(cache.names(7, entity(2)?, BucketGroup::new(1)?), None);
    assert_eq!(cache.names(7, entity(1)?, BucketGroup::new(1)?), Some(true));
    Ok(())
}

#[test]
fn test_a_set_larger_than_half_the_budget_is_not_held_and_drops_its_older_copy() -> TestResult {
    let mut cache = ReferrerCache::new(2 * cost(2));
    let o = entity(1)?;
    cache.put(7, o, set_of(&[1, 2])?);
    assert_eq!(cache.len(), 1);
    cache.put(7, o, set_of(&[1, 2, 3])?);
    assert!(cache.is_empty());
    assert_eq!(cache.names(7, o, BucketGroup::new(1)?), None);
    Ok(())
}

#[test]
fn test_forgetting_a_tenant_keeps_the_others_in_both_generations() -> TestResult {
    let mut cache = ReferrerCache::new(4 * cost(1));
    cache.put(7, entity(1)?, set_of(&[1])?);
    cache.put(8, entity(1)?, set_of(&[1])?);
    cache.put(7, entity(2)?, set_of(&[1])?);
    // 7/1 and 8/1 are now old, 7/2 young.
    cache.forget(7);
    assert_eq!(cache.len(), 1);
    // The young generation's bytes were freed with 7/2: two more one-member sets fit in it, and no rotation drops 8/1.
    cache.put(9, entity(1)?, set_of(&[1])?);
    cache.put(9, entity(2)?, set_of(&[1])?);
    assert_eq!(cache.len(), 3);
    assert_eq!(cache.names(8, entity(1)?, BucketGroup::new(1)?), Some(true));
    assert_eq!(cache.names(7, entity(1)?, BucketGroup::new(1)?), None);
    assert_eq!(cache.names(7, entity(2)?, BucketGroup::new(1)?), None);
    Ok(())
}
