/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The generation rule of partitions, "An entity's referrer set names where its inbound edges may be": each case's
//! expected generation worked out by hand from the rule, max(named, highest) + 1 for a new group or a named generation
//! at or above the current one, the current one answered unchanged below it.

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Epoch};
use ultragraphdb_store_quads::{ReferrerMember, ReferrerSet, Registered};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn group(g: u32) -> TestResult<BucketGroup> {
    Ok(BucketGroup::new(g)?)
}

fn epoch(e: u64) -> TestResult<Epoch> {
    Ok(Epoch::new(e)?)
}

fn generation(g: u64) -> TestResult<NonZeroU64> {
    NonZeroU64::new(g).context("a generation is at least 1")
}

fn member(g: u32, e: u64, n: u64) -> TestResult<ReferrerMember> {
    Ok(ReferrerMember::new(group(g)?, epoch(e)?, generation(n)?))
}

/// The set holding group 7 at generation 3, added at epoch 5.
fn holding_seven_at_three() -> TestResult<ReferrerSet> {
    let mut set = ReferrerSet::new();
    set.set(group(7)?, generation(3)?, epoch(5)?);
    Ok(set)
}

#[test]
fn test_the_empty_set_has_no_member() -> TestResult {
    let set = ReferrerSet::new();
    assert!(set.members().is_empty());
    assert_eq!(set.member(group(0)?), None);
    assert_eq!(set.at(Epoch::MAX).count(), 0);
    assert_eq!(set, ReferrerSet::default());
    Ok(())
}

#[test]
fn test_a_first_registration_adds_a_member() -> TestResult {
    // partitions, "A first registration adds a member".
    let mut set = ReferrerSet::new();
    let registered = set.register(group(7)?, 0, epoch(12)?);
    assert_eq!(
        registered,
        Registered {
            generation: generation(1)?,
            changed: true
        }
    );
    assert_eq!(set.members(), &[member(7, 12, 1)?]);
    assert_eq!(set.at(epoch(11)?).count(), 0, "added at 12, not seen at 11");
    assert_eq!(
        set.at(epoch(12)?).copied().collect::<Vec<_>>(),
        vec![member(7, 12, 1)?]
    );
    Ok(())
}

#[test]
fn test_a_repeated_registration_changes_nothing() -> TestResult {
    // partitions, "A repeated registration changes nothing".
    let mut set = holding_seven_at_three()?;
    let registered = set.register(group(7)?, 2, epoch(30)?);
    assert_eq!(
        registered,
        Registered {
            generation: generation(3)?,
            changed: false
        }
    );
    assert_eq!(set.members(), &[member(7, 5, 3)?]);
    Ok(())
}

#[test]
fn test_a_renewal_raises_the_generation_and_keeps_the_epoch() -> TestResult {
    // partitions, "A renewal raises the generation".
    let mut set = holding_seven_at_three()?;
    let registered = set.register(group(7)?, 3, epoch(30)?);
    assert_eq!(
        registered,
        Registered {
            generation: generation(4)?,
            changed: true
        }
    );
    assert_eq!(set.members(), &[member(7, 5, 4)?]);
    Ok(())
}

#[test]
fn test_a_named_generation_above_the_current_one_takes_one_above_it() -> TestResult {
    let mut set = holding_seven_at_three()?;
    let registered = set.register(group(7)?, 9, epoch(30)?);
    assert_eq!(registered.generation, generation(10)?);
    assert!(registered.changed);
    assert_eq!(set.members(), &[member(7, 5, 10)?]);
    Ok(())
}

#[test]
fn test_a_new_group_naming_a_generation_takes_one_above_it() -> TestResult {
    // A group not in the set, named generation 5: max(5, 0) + 1.
    let mut set = holding_seven_at_three()?;
    let registered = set.register(group(2)?, 5, epoch(30)?);
    assert_eq!(registered.generation, generation(6)?);
    assert!(registered.changed);
    assert_eq!(set.members(), &[member(2, 30, 6)?, member(7, 5, 3)?]);
    Ok(())
}

#[test]
fn test_the_largest_named_generation_saturates() -> TestResult {
    let mut set = ReferrerSet::new();
    let registered = set.register(group(1)?, u64::MAX, epoch(1)?);
    assert_eq!(registered.generation, generation(u64::MAX)?);
    Ok(())
}

#[test]
fn test_members_stay_sorted_by_group_one_per_group() -> TestResult {
    let mut set = ReferrerSet::new();
    for g in [9, 3, 7, 3] {
        let _registered: Registered = set.register(group(g)?, 0, epoch(u64::from(g))?);
    }
    let groups: Vec<u32> = set.members().iter().map(|m| m.group().get()).collect();
    assert_eq!(groups, vec![3, 7, 9]);
    assert_eq!(
        set.member(group(3)?),
        Some(&member(3, 3, 1)?),
        "the repeat named 0 below 1"
    );
    assert_eq!(set.member(group(4)?), None);
    Ok(())
}

#[test]
fn test_groups_at_both_ends_of_the_range_are_members() -> TestResult {
    let mut set = ReferrerSet::new();
    let last = BucketGroup::COUNT - 1;
    let _last: Registered = set.register(group(last)?, 0, epoch(1)?);
    let _first: Registered = set.register(group(0)?, 0, epoch(1)?);
    assert_eq!(set.members(), &[member(0, 1, 1)?, member(last, 1, 1)?]);
    Ok(())
}

#[test]
fn test_a_read_at_an_epoch_sees_the_members_added_at_or_below_it() -> TestResult {
    let mut set = ReferrerSet::new();
    set.set(group(1)?, generation(1)?, epoch(4)?);
    set.set(group(2)?, generation(1)?, epoch(8)?);
    set.set(group(3)?, generation(1)?, epoch(6)?);
    let at =
        |e| -> TestResult<Vec<u32>> { Ok(set.at(epoch(e)?).map(|m| m.group().get()).collect()) };
    assert_eq!(at(3)?, Vec::<u32>::new());
    assert_eq!(at(4)?, vec![1]);
    assert_eq!(at(7)?, vec![1, 3]);
    assert_eq!(at(8)?, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn test_set_adds_a_member_at_its_epoch_and_keeps_a_member_s_epoch() -> TestResult {
    let mut set = ReferrerSet::new();
    set.set(group(7)?, generation(4)?, epoch(12)?);
    assert_eq!(set.members(), &[member(7, 12, 4)?]);
    // A logged registration names the generation it set: a lower one is set as named, the epoch kept.
    set.set(group(7)?, generation(2)?, epoch(20)?);
    assert_eq!(set.members(), &[member(7, 12, 2)?]);
    set.set(group(1)?, generation(9)?, epoch(20)?);
    assert_eq!(set.members(), &[member(1, 20, 9)?, member(7, 12, 2)?]);
    Ok(())
}
