/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition as an entity's home: registrations getting or creating the entity's ID and their rounds writing the
//! referrer-set member at the commit version's epoch (term-dictionary, "A reference to another partition's entity
//! registers at its home"); the home's own groups added by the commit first referencing the entity from them
//! (partitions, "The home adds its own group without a registration"); and replay deriving the same referrer sets
//! (write-ahead-log, "Replay derives a home's referrer-set deltas").

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_apply::{Applied, ChangesetErrorEnum, EntityRef, Executed, LogicalWrites};
use ultragraphdb_store_quads::{QuadErrorEnum, ReferrerMember};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::VersionlessEntry;

use crate::utils::changesets::{
    NS, TestChangesets, changeset, changesets, ex, insert, stored_keys, uid, write,
};
use crate::utils::partitions::{
    bucket_of, epoch, group_of, owning_all_but, referrers_key, version,
};

fn generation(n: u64) -> TestResult<NonZeroU64> {
    NonZeroU64::new(n).context("a generation is at least 1")
}

fn member(group: BucketGroup, e: u64, n: u64) -> TestResult<ReferrerMember> {
    Ok(ReferrerMember::new(group, epoch(e)?, generation(n)?))
}

fn members(a: &TestChangesets, entity: SystemId, at: u64) -> TestResult<Vec<ReferrerMember>> {
    Ok(a.quads().referrers(NS, entity, at)?.members().to_vec())
}

/// `executed` applied alone in a round at `at`.
fn apply_at(a: &TestChangesets, at: u64, executed: Executed) -> TestResult {
    let _applied: Vec<Applied> = a.apply_round(vec![(at, executed)])?;
    Ok(())
}

#[test]
fn test_a_registration_of_a_new_iri_creates_its_id_and_its_round_adds_the_member() -> TestResult {
    // partitions, "A first registration adds a member": group 7 at epoch 12, generation 1.
    let a = changesets()?;
    let registering = a.register(NS, EntityRef::Iri(ex("o").as_str()))?;
    let o = registering.entity();
    assert_eq!(
        o.bucket(),
        bucket_of("o")?,
        "its bucket is derived from the IRI"
    );
    assert_eq!(
        a.quads().id_of(NS, ex("o").as_str())?,
        None,
        "not held before its round"
    );
    let seven = BucketGroup::new(7)?;
    let at = version(12, 2)?;
    apply_at(&a, at, registering.finish(seven, generation(1)?))?;
    assert_eq!(
        a.quads().id_of(NS, ex("o").as_str())?,
        Some(o),
        "its round writes its entries"
    );
    assert_eq!(members(&a, o, at)?, vec![member(seven, 12, 1)?]);
    assert!(
        members(&a, o, at - 2)?.is_empty(),
        "the member is versioned at the round's commit"
    );
    Ok(())
}

#[test]
fn test_a_registration_of_a_known_entity_by_id_gives_its_id() -> TestResult {
    let a = changesets()?;
    write(&a, &["<s> <knows> <o>"])?;
    let o = uid(&a, "o")?;
    let by_id = a.register(NS, EntityRef::Id(o))?;
    assert_eq!(by_id.entity(), o);
    let by_iri = a.register(NS, EntityRef::Iri(ex("o").as_str()))?;
    assert_eq!(by_iri.entity(), o, "a repeat gives the same ID");
    a.release(&by_iri.into_executed());
    a.release(&by_id.into_executed());
    Ok(())
}

#[test]
fn test_a_registration_of_an_id_the_dictionary_does_not_hold_is_corruption() -> TestResult {
    let a = changesets()?;
    let unknown = SystemId::new(TypePrefix::RESOURCE, bucket_of("o")?, Sequence::new(999)?);
    let refused = a
        .register(NS, EntityRef::Id(unknown))
        .err()
        .context("refused")?;
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Quad(q) if matches!(q.0, QuadErrorEnum::Corrupted { .. })),
        "not corruption: {refused:?}"
    );
    Ok(())
}

#[test]
fn test_a_registration_of_an_iri_another_partition_owns_is_refused() -> TestResult {
    let a = owning_all_but("o")?;
    let refused = a
        .register(NS, EntityRef::Iri(ex("o").as_str()))
        .err()
        .context("refused")?;
    let expected = bucket_of("o")?.get();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Quad(q)
            if matches!(&q.0, QuadErrorEnum::NotOwned { bucket, .. } if *bucket == expected)),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_a_released_registration_writes_nothing_and_leaves_no_entry() -> TestResult {
    let a = changesets()?;
    write(&a, &["<x> <knows> <y>"])?;
    let registering = a.register(NS, EntityRef::Iri(ex("o").as_str()))?;
    let o = registering.entity();
    let executed = registering.into_executed();
    assert!(
        executed.writes().versionless().is_empty(),
        "nothing to write"
    );
    a.release(&executed);
    // The ID was burned: the next creator takes another.
    let again = a.register(NS, EntityRef::Iri(ex("o").as_str()))?;
    assert_ne!(again.entity(), o);
    a.release(&again.into_executed());
    assert_eq!(a.quads().id_of(NS, ex("o").as_str())?, None);
    Ok(())
}

#[test]
fn test_two_registrations_of_one_entity_in_one_round_both_land() -> TestResult {
    let a = changesets()?;
    write(&a, &["<s> <knows> <o>"])?;
    let o = uid(&a, "o")?;
    let (seven, nine) = (BucketGroup::new(7)?, BucketGroup::new(9)?);
    let first = a
        .register(NS, EntityRef::Id(o))?
        .finish(seven, generation(1)?);
    let second = a
        .register(NS, EntityRef::Iri(ex("o").as_str()))?
        .finish(nine, generation(1)?);
    let (v1, v2) = (version(12, 2)?, version(13, 2)?);
    let _applied: Vec<Applied> = a.apply_round(vec![(v1, first), (v2, second)])?;
    let own = group_of("s")?;
    let mut expected = vec![member(seven, 12, 1)?, member(nine, 13, 1)?];
    // The home's own group of `<s>`, added by the commit of its edge, before both.
    let held = members(&a, o, v2)?;
    let own_member = held
        .iter()
        .find(|m| m.group() == own)
        .copied()
        .context("own group")?;
    expected.push(own_member);
    expected.sort_by_key(ReferrerMember::group);
    assert_eq!(held, expected);
    assert!(
        !members(&a, o, v1)?.iter().any(|m| m.group() == nine),
        "9 lands at v2"
    );
    Ok(())
}

#[test]
fn test_a_renewal_s_round_raises_the_generation_and_keeps_the_epoch() -> TestResult {
    // partitions, "A renewal raises the generation", as the round writes it.
    let a = changesets()?;
    let seven = BucketGroup::new(7)?;
    let registering = a.register(NS, EntityRef::Iri(ex("o").as_str()))?;
    let o = registering.entity();
    apply_at(
        &a,
        version(12, 2)?,
        registering.finish(seven, generation(3)?),
    )?;
    let renewed = a
        .register(NS, EntityRef::Id(o))?
        .finish(seven, generation(4)?);
    let at = version(20, 2)?;
    apply_at(&a, at, renewed)?;
    assert_eq!(members(&a, o, at)?, vec![member(seven, 12, 4)?]);
    Ok(())
}

#[test]
fn test_the_home_adds_its_own_group_in_the_commit_first_referencing_the_entity() -> TestResult {
    // partitions, "The home adds its own group without a registration": epoch 8, group of the subject.
    let a = changesets()?;
    write(&a, &["<x> <name> \"x\""])?;
    let executed = a.execute(&changeset(vec![insert("<s> <knows> <hub>")?]))?;
    assert!(
        !executed
            .reads()
            .lists()
            .iter()
            .any(|k| k.first() == Some(&Keyspace::Referrers.byte())),
        "the referrer set is no read: concurrent commits never conflict on it"
    );
    let at = version(8, 2)?;
    apply_at(&a, at, executed)?;
    let hub = uid(&a, "hub")?;
    assert_eq!(members(&a, hub, at)?, vec![member(group_of("s")?, 8, 1)?]);
    // A literal object has no referrer set, nor does the predicate.
    let knows = uid(&a, "knows")?;
    assert!(members(&a, knows, at)?.is_empty());
    Ok(())
}

#[test]
fn test_a_later_edge_from_the_same_group_keeps_the_member() -> TestResult {
    let a = changesets()?;
    let first = a.execute(&changeset(vec![insert("<s> <knows> <hub>")?]))?;
    apply_at(&a, version(8, 2)?, first)?;
    let later = a.execute(&changeset(vec![insert("<s> <likes> <hub>")?]))?;
    let at = version(9, 2)?;
    apply_at(&a, at, later)?;
    let hub = uid(&a, "hub")?;
    assert_eq!(members(&a, hub, at)?, vec![member(group_of("s")?, 8, 1)?]);
    Ok(())
}

#[test]
fn test_concurrent_first_references_from_one_group_add_it_once_without_conflict() -> TestResult {
    let a = changesets()?;
    write(&a, &["<x> <knows> <hub>"])?;
    let hub = uid(&a, "hub")?;
    let before = members(&a, hub, u64::MAX)?;
    // Both execute before either applies; both reference hub from s's group.
    let one = a.execute(&changeset(vec![insert("<s> <knows> <hub>")?]))?;
    let two = a.execute(&changeset(vec![insert("<s> <likes> <hub>")?]))?;
    let (v1, v2) = (version(30, 2)?, version(31, 2)?);
    let _applied: Vec<Applied> = a.apply_round(vec![(v1, one), (v2, two)])?;
    let own = group_of("s")?;
    let held = members(&a, hub, v2)?;
    let added: Vec<&ReferrerMember> = held.iter().filter(|m| m.group() == own).collect();
    if before.iter().any(|m| m.group() == own) {
        assert_eq!(added.len(), 1);
    } else {
        assert_eq!(
            added,
            vec![&member(own, 30, 1)?],
            "the first commit adds it"
        );
    }
    Ok(())
}

#[test]
fn test_an_object_another_partition_owns_joins_no_referrer_set_here() -> TestResult {
    let a = owning_all_but("foreign")?;
    let given = SystemId::new(
        TypePrefix::RESOURCE,
        bucket_of("foreign")?,
        Sequence::new(77)?,
    );
    let mut e = a.begin(NS, a.quads().lists().applied_ts())?;
    e.adopt(ex("foreign").as_str(), given);
    e.apply(&changeset(vec![insert("<a> <knows> <foreign>")?]))?;
    let executed = e.finish()?;
    assert!(
        !executed
            .writes()
            .entry_keys()
            .any(|k| k == referrers_key(given).as_slice()),
        "its home keeps its referrer set"
    );
    let at = a.quads().lists().next_version();
    apply_at(&a, at, executed)?;
    assert!(members(&a, given, at)?.is_empty());
    Ok(())
}

/// The dictionary entries `executed` writes.
fn dictionary(executed: &Executed) -> Vec<VersionlessEntry> {
    executed
        .writes()
        .versionless()
        .iter()
        .filter(|e| e.key().first() == Some(&Keyspace::Dictionary.byte()))
        .cloned()
        .collect()
}

#[test]
fn test_replay_derives_the_home_s_own_groups_alike() -> TestResult {
    let first = changesets()?;
    let replica = changesets()?;
    let executed = first.execute(&changeset(vec![
        insert("<s> <knows> <hub>")?,
        insert("<t> <knows> <hub>")?,
    ]))?;
    let logical: LogicalWrites = executed.logical()?;
    let entries = dictionary(&executed);
    let at = version(8, 2)?;
    apply_at(&first, at, executed)?;
    let borrowed: Vec<&VersionlessEntry> = entries.iter().collect();
    let replayed = replica.prepare_logical(NS, &logical, &borrowed)?;
    let _replayed: Vec<Applied> = replica.apply_round_with(vec![(at, replayed)], &entries, at)?;
    let hub = uid(&first, "hub")?;
    assert_eq!(members(&replica, hub, at)?, members(&first, hub, at)?);
    assert!(!members(&first, hub, at)?.is_empty());
    Ok(())
}

#[test]
fn test_replay_derives_a_home_s_registrations_at_their_versions() -> TestResult {
    // write-ahead-log, "Replay derives a home's referrer-set deltas".
    let home = changesets()?;
    let replica = changesets()?;
    let (seven, nine) = (BucketGroup::new(7)?, BucketGroup::new(9)?);
    let registering = home.register(NS, EntityRef::Iri(ex("o").as_str()))?;
    let o = registering.entity();
    let first = registering.finish(seven, generation(1)?);
    let second = home
        .register(NS, EntityRef::Iri(ex("o").as_str()))?
        .finish(nine, generation(2)?);
    let (v1, v2) = (version(12, 2)?, version(14, 2)?);
    let _applied: Vec<Applied> = home.apply_round(vec![(v1, first), (v2, second)])?;
    let replayed = vec![
        (
            v1,
            replica.prepare_replayed_registration(NS, o, seven, generation(1)?),
        ),
        (
            v2,
            replica.prepare_replayed_registration(NS, o, nine, generation(2)?),
        ),
    ];
    let _replayed: Vec<Applied> = replica.apply_round(replayed)?;
    for at in [v1, v2] {
        assert_eq!(members(&replica, o, at)?, members(&home, o, at)?);
    }
    assert_eq!(members(&replica, o, v1)?, vec![member(seven, 12, 1)?]);
    assert_eq!(
        members(&replica, o, v2)?,
        vec![member(seven, 12, 1)?, member(nine, 14, 2)?]
    );
    Ok(())
}

/// A registration holds entries to make durable when it created the ID, and none when the entity's are already durable.
#[test]
fn test_a_registration_holds_entries_only_of_an_id_it_created() -> TestResult {
    let a = changesets()?;
    write(&a, &["<s> <knows> <o>"])?;
    let created = a.register(NS, EntityRef::Iri(ex("new").as_str()))?;
    assert!(created.holds_entries());
    a.release(&created.into_executed());
    let known = a.register(NS, EntityRef::Id(uid(&a, "o")?))?;
    assert!(!known.holds_entries());
    a.release(&known.into_executed());
    Ok(())
}

/// A literal object's posting names no entity: a commit of literals alone adds no referrer set.
#[test]
fn test_a_literal_object_adds_no_referrer_set() -> TestResult {
    let a = changesets()?;
    let executed = a.execute(&changeset(vec![
        insert("<s> <name> \"Ann\"")?,
        insert("<s> <age> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer>")?,
    ]))?;
    apply_at(&a, version(8, 2)?, executed)?;
    assert!(
        !stored_keys(&a)
            .iter()
            .any(|key| key.first() == Some(&Keyspace::Referrers.byte()))
    );
    Ok(())
}
