/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The referrer sets a round takes and leaves in memory: a round holds every set it took as the store holds it once its
//! batch is written, and an execution and an apply take a held set in place of the store's.

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Epoch, SystemId};
use ultragraphdb_store_quads::{ReferrerMember, ReferrerSet};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::changesets::{TENANT, TestChangesets, changeset, changesets, insert, uid, write};

/// A store holding `<hub>` and `<s>`, neither referencing the other, with `<s>`'s bucket group.
fn hub_and_subject() -> TestResult<(TestChangesets, SystemId, BucketGroup)> {
    let a = changesets()?;
    write(&a, &["<hub> <name> \"h\"", "<s> <name> \"x\""])?;
    let hub = uid(&a, "hub")?;
    let group = BucketGroup::of(uid(&a, "s")?.bucket());
    Ok((a, hub, group))
}

/// The groups of `entity`'s referrer set as the store holds it at the watermark.
fn stored_groups(a: &TestChangesets, entity: SystemId) -> TestResult<Vec<BucketGroup>> {
    Ok(a.quads()
        .referrers(TENANT, entity, a.quads().lists().applied_ts())?
        .members()
        .iter()
        .map(ReferrerMember::group)
        .collect())
}

/// A set naming `group` at generation 1 and epoch 1.
fn naming(group: BucketGroup) -> TestResult<ReferrerSet> {
    let mut set = ReferrerSet::new();
    set.set(group, NonZeroU64::MIN, Epoch::new(1)?);
    Ok(set)
}

#[test]
fn test_a_round_holds_the_referrer_sets_it_writes_as_the_store_holds_them() -> TestResult {
    let (a, hub, group) = hub_and_subject()?;
    assert!(a.quads().held_referrers(TENANT, hub).is_none());
    write(&a, &["<s> <knows> <hub>"])?;
    let held = a.quads().held_referrers(TENANT, hub);
    let stored = a
        .quads()
        .referrers(TENANT, hub, a.quads().lists().applied_ts())?;
    assert_eq!(held, Some(stored));
    assert_eq!(stored_groups(&a, hub)?, vec![group]);
    Ok(())
}

#[test]
fn test_an_execution_takes_a_held_set_naming_its_group_as_named() -> TestResult {
    // The store holds no set for <hub>: the edge is a candidate, until a held set names <s>'s group.
    let (a, hub, group) = hub_and_subject()?;
    let edge = changeset(vec![insert("<s> <knows> <hub>")?]);
    let unheld = a.execute(&edge)?;
    assert_eq!(unheld.referrers(), &[(hub, group)]);
    a.release(&unheld);
    a.quads().hold_referrers(TENANT, hub, naming(group)?);
    let held = a.execute(&edge)?;
    assert!(held.referrers().is_empty());
    a.release(&held);
    Ok(())
}

#[test]
fn test_an_apply_adds_to_the_held_set_in_place_of_the_stored_one() -> TestResult {
    // The held set names another group than <s>'s; the store holds none. The written set is the held one with <s>'s
    // group added.
    let (a, hub, group) = hub_and_subject()?;
    let other = BucketGroup::new((group.get() + 1) % 1024)?;
    a.quads().hold_referrers(TENANT, hub, naming(other)?);
    write(&a, &["<s> <knows> <hub>"])?;
    let mut expected = vec![group, other];
    expected.sort();
    assert_eq!(stored_groups(&a, hub)?, expected);
    Ok(())
}
