/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_cluster_partition::{Home, PartitionError, Reference};
use ultragraphdb_core_ids::{BucketGroup, Version};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::cluster::{Cluster, NS, ex, group, pid};

/// A reference to `<name>`, homed in `home`, from `group`, having seen `named`, by its IRI.
fn reference(home: u32, name: &str, group: BucketGroup, named: u64) -> TestResult<Reference> {
    Ok(Reference::new(
        pid(home)?,
        NS,
        ex(name).as_str().to_string(),
        None,
        Some(group),
        named,
    ))
}

/// The first registration of a new IRI creates its ID at the home and adds the group at generation 1, answered once
/// durable with the close below the epoch it was stamped at: the home's latest delivered close.
#[test]
fn test_a_first_registration_creates_the_id_and_adds_the_member() -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let b = cluster.name_in(2, "b")?;
    let g = group(&cluster.name_in(1, "a")?)?;
    let delivered = cluster.delivered(2)?;
    let answer = cluster.register(&reference(2, &b, g, 0)?)?;
    let held = cluster.id_at(2, &b)?.context("the home holds the ID")?;
    assert_eq!(answer.entity(), held);
    assert_eq!(answer.generation(), NonZeroU64::new(1));
    assert_eq!(answer.close(), delivered);
    let quads = cluster.part(2)?.applier().quads();
    let set = quads.referrers(NS, held, u64::MAX)?;
    let member = set.member(g).context("the member")?;
    assert_eq!(member.generation().get(), 1);
    assert_eq!(
        member.epoch(),
        delivered.next()?,
        "stamped at the open epoch"
    );
    Ok(())
}

/// A registration naming the current generation renews it, keeping the epoch; one naming an older generation is a
/// repeat, answered at once with the current generation and changing nothing.
#[test]
fn test_a_renewal_raises_the_generation_and_a_repeat_changes_nothing() -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let b = cluster.name_in(2, "b")?;
    let g = group(&cluster.name_in(1, "a")?)?;
    let first = cluster.register(&reference(2, &b, g, 0)?)?;
    let quads = cluster.part(2)?.applier().quads();
    let added = quads.referrers(NS, first.entity(), u64::MAX)?;
    let renewed = cluster.register(&reference(2, &b, g, 1)?)?;
    assert_eq!(renewed.generation(), NonZeroU64::new(2));
    let after = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, first.entity(), u64::MAX)?;
    assert_eq!(
        after.member(g).map(|m| m.epoch()),
        added.member(g).map(|m| m.epoch()),
        "a renewal keeps the epoch"
    );
    let watermark = cluster.part(2)?.applier().quads().lists().applied_ts();
    let repeat = cluster.register(&reference(2, &b, g, 1)?)?;
    assert_eq!(repeat.generation(), NonZeroU64::new(2));
    assert_eq!(repeat.entity(), first.entity());
    assert_eq!(
        cluster.part(2)?.applier().quads().lists().applied_ts(),
        watermark,
        "a repeat writes nothing"
    );
    Ok(())
}

/// Two registrations of one new IRI from two groups, both accepted before either is durable, share one ID and each
/// add its member.
#[test]
fn test_concurrent_registrants_share_one_id() -> TestResult {
    let mut cluster = Cluster::even(3)?;
    let b = cluster.name_in(2, "b")?;
    let g1 = group(&cluster.name_in(1, "a")?)?;
    let g3 = group(&cluster.name_in(3, "c")?)?;
    let answers = cluster.register_all(2, &[reference(2, &b, g1, 0)?, reference(2, &b, g3, 0)?])?;
    let ids: Vec<_> = answers.iter().map(|a| a.entity()).collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids.first(), ids.get(1), "one ID");
    let held = cluster.id_at(2, &b)?.context("the home holds the ID")?;
    assert_eq!(ids.first(), Some(&held));
    let set = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, held, u64::MAX)?;
    let mut groups: Vec<u32> = set.members().iter().map(|m| m.group().get()).collect();
    let mut expected = vec![g1.get(), g3.get()];
    groups.sort_unstable();
    expected.sort_unstable();
    assert_eq!(groups, expected);
    Ok(())
}

/// A registration naming no group creates the ID durably and adds no member.
#[test]
fn test_a_predicate_registration_adds_no_member() -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let knows = cluster.name_in(2, "knows")?;
    let answer = cluster.register(&Reference::new(
        pid(2)?,
        NS,
        ex(&knows).as_str().to_string(),
        None,
        None,
        0,
    ))?;
    assert_eq!(answer.generation(), None);
    assert_eq!(cluster.id_at(2, &knows)?, Some(answer.entity()));
    let set = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, answer.entity(), u64::MAX)?;
    assert!(set.members().is_empty());
    Ok(())
}

/// A home refuses a registration for an entity of a bucket it does not own, naming the bucket and its owner.
#[test]
fn test_a_home_refuses_another_partitions_entity() -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let bucket = crate::utils::cluster::bucket(&a)?;
    let refused = cluster.register(&reference(2, &a, group(&a)?, 0)?);
    let error = refused.err().context("a refusal")?;
    assert!(
        error
            .to_string()
            .contains(&PartitionError::wrong_partition(bucket.get(), 1).to_string()),
        "{error}"
    );
    Ok(())
}

/// A new home waits on nothing, and settling replies it does not wait on leaves them.
#[test]
fn test_settling_leaves_replies_the_home_does_not_wait_on() {
    let mut home = Home::new();
    assert!(home.is_idle());
    let mut replies = vec![ultragraphdb_transactions_commit::Reply::new(
        5,
        Ok(Version::from_u64(8).to_u64()),
    )];
    let mut out = Vec::new();
    home.settle(&mut replies, &mut out);
    assert_eq!(replies.len(), 1);
    assert!(out.is_empty());
}
