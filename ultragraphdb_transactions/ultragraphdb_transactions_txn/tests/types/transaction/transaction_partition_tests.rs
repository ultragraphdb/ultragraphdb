/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A transaction as one partition's (partitions, "A read-write transaction stays within one partition in this stage";
//! term-dictionary, "A reference to another partition's entity registers at its home"): its reverse reads and index
//! lookups allowed only while its partition owns the namespace's whole run, a forward read or a write of a subject another
//! partition owns aborting it, an existing blank node checked by its skolem IRI's bucket, a plain write of a unique
//! value another partition owns aborting it, and the IDs and marks of other partitions' entities its homes answered,
//! each raising its epoch floor and its marks listed in its record's section 4. The placement map and the bucket
//! derivation are the oracle of every owner.

use std::num::NonZeroU64;

use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_cluster_placement::{NamespaceRun, NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::{
    Bucket, BucketGroup, CanonicalKey, Epoch, Namespace, SystemId, TypePrefix,
};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{RecordRegistration, WriteSetRecordView};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_txn::{Mode, TxnError, TxnErrorEnum, TxnLimits};

use crate::utils::fixture::{
    Applier, NS, SECOND, Txn, applier, begin, begin_with, changeset, ex, held, id_in, insert,
    insert_literal, objects, prepared, store,
};

fn placement(partitions: u32) -> TestResult<PlacementMap> {
    PlacementMap::even(partitions, NodeId::new(1)).context("the placement")
}

fn partition(n: u32) -> TestResult<PartitionId> {
    PartitionId::new(n).context("the partition")
}

fn epoch(n: u64) -> TestResult<Epoch> {
    Epoch::new(n).context("the epoch")
}

/// The bucket of the IRI `<name>`, as the system-identity derivation gives it.
fn bucket_of(name: &str) -> TestResult<Bucket> {
    let iri = ex(name);
    let key = CanonicalKey::iri(iri.as_str()).context("the key")?;
    Ok(Bucket::derive(
        Namespace::new(NS),
        TypePrefix::RESOURCE,
        &key,
    ))
}

/// The name `s{i}` of the first `i` whose IRI's bucket `placement` gives to `partition` at epoch 0.
fn owned_by(placement: &PlacementMap, partition: u32) -> TestResult<String> {
    for i in 0..1_000 {
        let name = format!("s{i}");
        if placement.partition_of(bucket_of(&name)?, Epoch::ZERO).get() == partition {
            return Ok(name);
        }
    }
    Err(TestError::new(format!("no name of partition {partition}")))
}

/// The partitions other than `own` the layout of `placement` at epoch 0 assigns buckets of the namespace's run to, read
/// off its ranges: the oracle of the partition a fan-out read would reach.
fn others_of_run(placement: &PlacementMap, own: u32) -> Vec<u32> {
    let run = NamespaceRun::of(Namespace::new(NS)).range();
    let mut others: Vec<u32> = placement
        .ranges_at(Epoch::ZERO)
        .into_iter()
        .filter(|(range, _)| range.start() < run.end() && run.start() < range.end())
        .map(|(_, p)| p.get())
        .filter(|p| *p != own)
        .collect();
    others.sort_unstable();
    others.dedup();
    others
}

/// The subjects of `predicate` pointing at `<object>` that `txn` reads.
fn subjects(
    txn: &mut Txn<'_>,
    predicate: &str,
    object: &str,
) -> Result<Vec<NamedOrBlankNode>, TxnError> {
    let mut out = vec![NamedOrBlankNode::from(ex("stale"))];
    let (p, o) = (ex(predicate), ex(object));
    txn.subjects_into(
        GraphName::DefaultGraph.as_ref(),
        p.as_ref(),
        o.as_ref().into(),
        0,
        &mut out,
    )?;
    Ok(out)
}

/// The subjects whose `predicate` holds `value` by its `term` index that `txn` reads.
fn lookup(
    txn: &mut Txn<'_>,
    predicate: &str,
    value: &str,
) -> Result<Vec<NamedOrBlankNode>, TxnError> {
    let mut out = vec![NamedOrBlankNode::from(ex("stale"))];
    let p = ex(predicate);
    let v = Literal::new_simple_literal(value);
    txn.lookup_into(
        GraphName::DefaultGraph.as_ref(),
        p.as_ref(),
        "term",
        v.as_ref().into(),
        0,
        &mut out,
    )?;
    Ok(out)
}

/// The ID a writer of `<name>` is given now, its proposal released at once: the ID another write holds, or a new one.
fn id_now(applier: &Applier, name: &str) -> TestResult<SystemId> {
    let mut writer = begin(applier, Mode::ReadWrite)?;
    writer
        .write(&changeset(vec![insert(name, "p", "q")]), 0)
        .context("the writer writes the IRI")?;
    let proposal = prepared(writer.commit(1, 1, &[], 0))?;
    let id = id_in(&proposal, name)?;
    let _buffers = proposal.release(applier);
    Ok(id)
}

/// The cases ending a transaction cross-partition, each with what it asks of a transaction bound to partition 1 of two.
fn assert_ends_cross_partition(
    txn: &mut Txn<'_>,
    error: Option<TxnError>,
    other: u32,
) -> TestResult {
    let error = error.context("refused")?;
    assert_eq!(
        error.0,
        TxnErrorEnum::CrossPartition {
            partition: 1,
            other
        }
    );
    assert!(!error.is_retryable());
    assert!(txn.is_ended(), "it is aborted");
    assert_eq!(
        objects(txn, "a", "knows", 0).err(),
        Some(TxnError::ended()),
        "every later request is refused"
    );
    Ok(())
}

// --- Reverse reads and index lookups ---------------------------------------------------------------------------------

#[test]
fn test_an_unbound_transaction_reads_the_subjects_pointing_at_an_object() -> TestResult {
    let applier = applier()?;
    store(
        &applier,
        vec![
            insert("x", "knows", "b"),
            insert("y", "knows", "b"),
            insert("z", "likes", "b"),
        ],
    )?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    // The transaction's own write is seen on top of the store.
    txn.write(&changeset(vec![insert("w", "knows", "b")]), 0)?;
    let mut found = subjects(&mut txn, "knows", "b")?;
    found.sort_by_key(ToString::to_string);
    let mut expected: Vec<NamedOrBlankNode> =
        ["w", "x", "y"].iter().map(|n| ex(n).into()).collect();
    expected.sort_by_key(ToString::to_string);
    assert_eq!(
        found, expected,
        "cleared first, then the subjects of `knows`"
    );
    Ok(())
}

#[test]
fn test_a_reverse_read_is_recorded_in_the_read_set() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("x", "knows", "b")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let _found = subjects(&mut txn, "knows", "b")?;
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    assert!(
        proposal.read_set().lists().count() > 0,
        "the object's lists it read are in its read set"
    );
    Ok(())
}

#[test]
fn test_a_bound_transaction_owning_the_whole_run_reads_its_reverse_entries_and_commits()
-> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("x", "knows", "b")])?;
    let one = placement(1)?;
    assert!(
        others_of_run(&one, 1).is_empty(),
        "partition 1 owns the run"
    );
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &one);
    assert_eq!(subjects(&mut txn, "knows", "b")?, vec![ex("x").into()]);
    txn.write(&changeset(vec![insert_literal("x", "seen", "true")]), 0)?;
    assert!(!txn.is_ended());
    let _proposal = prepared(txn.commit(1, 1, &[], 0))?;
    Ok(())
}

#[test]
fn test_a_reverse_read_over_a_spread_run_aborts_naming_the_lowest_other_partition() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("x", "knows", "b")])?;
    for (partitions, bound) in [(2, 1), (3, 2), (3, 3)] {
        let map = placement(partitions)?;
        let lowest = others_of_run(&map, bound)
            .first()
            .copied()
            .context("another partition holds part of the run")?;
        let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(bound)?, &map);
        let error = subjects(&mut txn, "knows", "b").err();
        assert_eq!(
            error,
            Some(TxnError::cross_partition(bound, lowest)),
            "{partitions} partitions, bound to {bound}"
        );
        assert!(txn.is_ended());
    }
    Ok(())
}

#[test]
fn test_a_reverse_read_over_a_spread_run_aborts_and_writes_nothing() -> TestResult {
    let applier = applier()?;
    let two = placement(2)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &two);
    let error = subjects(&mut txn, "knows", "b").err();
    assert_ends_cross_partition(&mut txn, error, 2)?;
    assert_eq!(
        txn.commit(1, 1, &[], 0).err(),
        Some(TxnError::ended()),
        "nothing is committed"
    );
    Ok(())
}

#[test]
fn test_a_first_lookup_over_a_spread_run_aborts_and_holds_nothing() -> TestResult {
    let applier = applier()?;
    let two = placement(2)?;
    let own = owned_by(&two, 1)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &two);
    let error = lookup(&mut txn, "name", "x").err();
    assert_ends_cross_partition(&mut txn, error, 2)?;
    // A transaction ended so writes nothing, here or later.
    assert_eq!(
        txn.write(&changeset(vec![insert(&own, "knows", "b")]), 0)
            .err(),
        Some(TxnError::ended())
    );
    assert!(held(&applier, &own, "knows")?.is_empty());
    Ok(())
}

#[test]
fn test_a_lookup_finds_the_subjects_whose_value_matches_by_its_index() -> TestResult {
    let applier = applier()?;
    let declared = PredicateDeclaration::new(ex("name")).with_tokenizers(vec!["term".into()]);
    store(
        &applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    )?;
    store(
        &applier,
        vec![
            insert_literal("x", "name", "ada"),
            insert_literal("y", "name", "bob"),
        ],
    )?;
    let one = placement(1)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &one);
    assert_eq!(lookup(&mut txn, "name", "ada")?, vec![ex("x").into()]);
    assert!(!txn.is_ended());
    Ok(())
}

#[test]
fn test_a_reverse_read_past_the_limit_ends_the_transaction() -> TestResult {
    let applier = applier()?;
    let limits = TxnLimits::default();
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
    let past = limits.read_write().as_micros() as u64 + 1;
    let (p, o) = (ex("knows"), ex("b"));
    let mut out = Vec::new();
    let error = txn
        .subjects_into(
            GraphName::DefaultGraph.as_ref(),
            p.as_ref(),
            o.as_ref().into(),
            past,
            &mut out,
        )
        .err();
    assert_eq!(error, Some(TxnError::past_limit(10_000)));
    assert!(txn.is_ended());
    assert_eq!(
        subjects(&mut txn, "knows", "b").err(),
        Some(TxnError::ended())
    );
    Ok(())
}

#[test]
fn test_a_cross_partition_read_releases_what_the_transaction_created() -> TestResult {
    let applier = applier()?;
    let two = placement(2)?;
    let new = owned_by(&two, 1)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &two);
    txn.write(&changeset(vec![insert(&new, "p", "q")]), 0)?;
    let created = id_now(&applier, &new)?;
    assert_eq!(
        id_now(&applier, &new)?,
        created,
        "the open transaction holds it"
    );
    let _refused = subjects(&mut txn, "knows", "b").err().context("refused")?;
    assert_ne!(id_now(&applier, &new)?, created, "the refusal released it");
    assert!(held(&applier, &new, "p")?.is_empty(), "nothing is written");
    Ok(())
}

// --- Forward reads and writes of another partition's subjects ---------------------------------------------------------

#[test]
fn test_a_forward_read_of_another_partitions_subject_aborts_after_an_own_one() -> TestResult {
    let applier = applier()?;
    let two = placement(2)?;
    let (alice, bob) = (owned_by(&two, 1)?, owned_by(&two, 2)?);
    store(
        &applier,
        vec![
            insert_literal(&alice, "on-call", "true"),
            insert_literal(&bob, "on-call", "true"),
        ],
    )?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &two);
    assert_eq!(
        objects(&mut txn, &alice, "on-call", 0)?,
        vec![Term::from(Literal::new_simple_literal("true"))]
    );
    let error = objects(&mut txn, &bob, "on-call", 0).err();
    assert_ends_cross_partition(&mut txn, error, 2)?;
    Ok(())
}

#[test]
fn test_an_unbound_transaction_reads_every_subject() -> TestResult {
    let applier = applier()?;
    let two = placement(2)?;
    let bob = owned_by(&two, 2)?;
    store(&applier, vec![insert_literal(&bob, "on-call", "true")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(objects(&mut txn, &bob, "on-call", 0)?.len(), 1);
    Ok(())
}

/// The skolem IRI of the blank node `txn`'s commit created, from the terms its execution wrote, and the bucket its tail
/// states: the six hexadecimal digits after `/.well-known/genid/`, read by hand.
fn created_blank(applier: &Applier) -> TestResult<(NamedNode, Bucket)> {
    let mut txn = begin(applier, Mode::ReadWrite)?;
    let blank = Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: oxrdf::BlankNode::new_unchecked("fresh").into(),
        predicate: ex("knows"),
        object: ex("b").into(),
    };
    txn.write(&changeset(vec![blank]), 0)?;
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    let skolem = proposal
        .executed()
        .terms()
        .written()
        .map(|(term, _)| term.to_string())
        .find(|term| term.contains("/.well-known/genid/"))
        .context("the commit names its blank node by a skolem IRI")?;
    let (executed, _buffers) = proposal.into_parts();
    let _applied = applier
        .apply_round(vec![(applier.quads().lists().applied_ts() + 1, executed)])
        .context("the round applies")?;
    let tail = skolem
        .rsplit('/')
        .next()
        .context("the IRI has a tail")?
        .get(..6)
        .context("six digits")?;
    let bucket =
        Bucket::new(u32::from_str_radix(tail, 16).context("hexadecimal")?).context("a bucket")?;
    Ok((NamedNode::new_unchecked(skolem), bucket))
}

/// An existing blank node is the partition's that owns the bucket its skolem IRI states, not the bucket the IRI's text
/// would derive: with two partitions one of the two bindings below disagrees with the derivation, whichever the IRI.
#[test]
fn test_an_existing_blank_node_is_checked_by_its_skolem_iris_bucket() -> TestResult {
    let applier = applier()?;
    let (skolem, bucket) = created_blank(&applier)?;
    let two = placement(2)?;
    let owner = two.partition_of(bucket, Epoch::ZERO).get();
    let other = 3 - owner;
    let write = Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: skolem.clone().into(),
        predicate: ex("knows"),
        object: ex("c").into(),
    };
    let mut own = begin(&applier, Mode::ReadWrite)?.bind(partition(owner)?, &two);
    own.write(&changeset(vec![write.clone()]), 0)?;
    assert!(!own.is_ended(), "its owner writes it");
    let mut out = Vec::new();
    own.objects_into(
        GraphName::DefaultGraph.as_ref(),
        skolem.as_ref().into(),
        ex("knows").as_ref(),
        0,
        &mut out,
    )?;
    assert!(!out.is_empty(), "and reads it");
    let mut foreign = begin(&applier, Mode::ReadWrite)?.bind(partition(other)?, &two);
    assert_eq!(
        foreign.write(&changeset(vec![write]), 0).err(),
        Some(TxnError::cross_partition(other, owner))
    );
    assert!(foreign.is_ended());
    let mut reader = begin(&applier, Mode::ReadWrite)?.bind(partition(other)?, &two);
    let refused = reader
        .objects_into(
            GraphName::DefaultGraph.as_ref(),
            skolem.as_ref().into(),
            ex("knows").as_ref(),
            0,
            &mut out,
        )
        .err();
    assert_eq!(refused, Some(TxnError::cross_partition(other, owner)));
    Ok(())
}

// --- Unique values ----------------------------------------------------------------------------------------------------

/// Declares `epi.id` unique.
fn unique_id(applier: &Applier) -> TestResult {
    let declared = PredicateDeclaration::new(ex("epi.id")).with_unique(true);
    store(
        applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    )
}

/// The owner, among two partitions, of the bucket of `epi.id` `value` (`Bucket::of_value` over the predicate's ID and
/// the value's encoding).
fn value_owner(applier: &Applier, placement: &PlacementMap, value: &str) -> TestResult<u32> {
    let predicate = applier
        .quads()
        .id_of(NS, ex("epi.id").as_str())
        .context("the dictionary reads")?
        .context("declared")?;
    let literal = Literal::new_simple_literal(value);
    let encoded = EncodedTerm::literal::<TestError>(literal.as_ref(), |_| {
        Err(TestError::new("a plain literal names no datatype"))
    })?;
    let bucket = Bucket::of_value(Namespace::new(NS), predicate, encoded.term().as_bytes());
    Ok(placement.partition_of(bucket, Epoch::ZERO).get())
}

/// A value of `epi.id` whose bucket `owner` owns among two partitions.
fn value_of(applier: &Applier, placement: &PlacementMap, owner: u32) -> TestResult<String> {
    for i in 0..1_000 {
        let value = format!("v{i}");
        if value_owner(applier, placement, &value)? == owner {
            return Ok(value);
        }
    }
    Err(TestError::new(format!("no value of partition {owner}")))
}

#[test]
fn test_a_plain_write_of_a_unique_value_another_partition_owns_aborts() -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let two = placement(2)?;
    let a = owned_by(&two, 1)?;
    let value = value_of(&applier, &two, 2)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &two);
    let error = txn
        .write(&changeset(vec![insert_literal(&a, "epi.id", &value)]), 0)
        .err();
    assert_ends_cross_partition(&mut txn, error, 2)?;
    assert!(held(&applier, &a, "epi.id")?.is_empty(), "nothing is held");
    Ok(())
}

#[test]
fn test_a_plain_write_of_a_unique_value_the_partition_owns_is_taken() -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let two = placement(2)?;
    let a = owned_by(&two, 1)?;
    let value = value_of(&applier, &two, 1)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &two);
    txn.write(&changeset(vec![insert_literal(&a, "epi.id", &value)]), 0)?;
    assert!(!txn.is_ended());
    // A value of a predicate not declared unique claims nothing, whoever owns its bucket.
    let foreign = value_of(&applier, &two, 2)?;
    txn.write(&changeset(vec![insert_literal(&a, "label", &foreign)]), 0)?;
    assert!(!txn.is_ended());
    Ok(())
}

// --- Adopted IDs, marks and the epoch floor --------------------------------------------------------------------------

/// The system ID of an entity homed elsewhere, by its layout: type 1, sequence `sequence`, bucket 5.
fn foreign_id(sequence: u64) -> TestResult<SystemId> {
    SystemId::from_u128((1 << 112) | (u128::from(sequence) << 48) | (5 << 24)).context("the ID")
}

fn generation(n: u64) -> TestResult<NonZeroU64> {
    NonZeroU64::new(n).context("a generation")
}

fn group(n: u32) -> TestResult<BucketGroup> {
    BucketGroup::new(n).context("the group")
}

#[test]
fn test_a_transaction_without_a_reply_has_no_epoch_floor() -> TestResult {
    let applier = applier()?;
    let txn = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(txn.epoch_floor(), None);
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(proposal.epoch_floor(), None);
    Ok(())
}

#[test]
fn test_the_epoch_floor_is_one_above_the_latest_close_a_reply_named() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let b = foreign_id(40)?;
    txn.adopt(ex("b").as_str(), b, epoch(11)?, 0)?;
    assert_eq!(txn.epoch_floor(), Some(epoch(12)?));
    // A reply naming an earlier close lowers nothing; a later one raises it.
    txn.mark(foreign_id(41)?, group(7)?, generation(1)?, epoch(9)?, 0)?;
    assert_eq!(txn.epoch_floor(), Some(epoch(12)?));
    txn.mark(foreign_id(42)?, group(7)?, generation(2)?, epoch(14)?, 0)?;
    assert_eq!(txn.epoch_floor(), Some(epoch(15)?));
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(proposal.epoch_floor(), Some(epoch(15)?));
    Ok(())
}

#[test]
fn test_an_adopted_iri_resolves_to_its_homes_id() -> TestResult {
    let applier = applier()?;
    let b = foreign_id(40)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.adopt(ex("b").as_str(), b, epoch(3)?, 0)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    let record = WriteSetRecordView::new(proposal.record())?;
    let quad = record.inserted().iter().next().context("one quad")?;
    // The quad's key ends in the object's encoding: an ID's 16 big-endian bytes after its tag.
    assert!(
        quad.key().ends_with(&b.to_be_bytes()),
        "the statement names the home's ID"
    );
    Ok(())
}

#[test]
fn test_a_mark_is_read_written_and_listed_in_section_4() -> TestResult {
    let applier = applier()?;
    let (entity, g, gen4) = (foreign_id(40)?, group(7)?, generation(4)?);
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(txn.read_mark(entity, g, 0)?, None, "no mark yet");
    txn.mark(entity, g, gen4, epoch(5)?, 0)?;
    assert_eq!(
        txn.read_mark(entity, g, 0)?,
        Some(gen4),
        "it sees its own mark"
    );
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    let record = WriteSetRecordView::new(proposal.record())?;
    assert_eq!(record.registration_count(), 1);
    assert_eq!(
        record.registration(0),
        Some(RecordRegistration::new(entity, g, gen4))
    );
    let key = QuadKey::Mark {
        namespace: NS,
        entity,
        group: g.get(),
    }
    .to_bytes();
    assert!(
        proposal.read_set().lists().any(|k| k == key.as_slice()),
        "the mark's key is in the read set"
    );
    Ok(())
}

#[test]
fn test_a_transaction_without_marks_lists_no_registration() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(
        WriteSetRecordView::new(proposal.record())?.registration_count(),
        0
    );
    Ok(())
}

#[test]
fn test_replies_to_an_ended_or_expired_transaction_are_refused() -> TestResult {
    let applier = applier()?;
    let (entity, g) = (foreign_id(40)?, group(7)?);
    let past = 10 * SECOND + 1;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(
        txn.adopt(ex("b").as_str(), entity, epoch(3)?, past).err(),
        Some(TxnError::past_limit(10_000))
    );
    assert!(txn.is_ended());
    assert_eq!(
        txn.adopt(ex("b").as_str(), entity, epoch(3)?, 0).err(),
        Some(TxnError::ended())
    );
    assert_eq!(
        txn.mark(entity, g, generation(1)?, epoch(3)?, 0).err(),
        Some(TxnError::ended())
    );
    assert_eq!(txn.read_mark(entity, g, 0).err(), Some(TxnError::ended()));
    let mut marking = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(
        marking
            .mark(entity, g, generation(1)?, epoch(3)?, past)
            .err(),
        Some(TxnError::past_limit(10_000))
    );
    let mut reading = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(
        reading.read_mark(entity, g, past).err(),
        Some(TxnError::past_limit(10_000))
    );
    assert_eq!(txn.epoch_floor(), None, "a refused reply raises nothing");
    Ok(())
}
