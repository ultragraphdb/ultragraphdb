/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The dictionary of a partition owning some buckets: an IRI of another partition's bucket is refused unless the
//! partition holds it or copied its ID from its home (term-dictionary, "A reference to another partition's entity
//! registers at its home"), in every position of a statement.

use oxrdf::{BlankNode, GraphName, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_ids::{Bucket, CanonicalKey, Namespace, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_store_quads::{OwnedBuckets, QuadErrorEnum, QuadStore, WriteTerms};
use ultragraphdb_store_sim::SimStore;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::helpers::{TestQuads, blocks, ex, lists_over, quad, quads_over, sim, write};

const NS: u64 = 0;

/// The bucket of `iri` in namespace 0, as core_ids derives it.
fn bucket_of(iri: &NamedNode) -> TestResult<Bucket> {
    Ok(Bucket::derive(
        Namespace::new(NS),
        TypePrefix::RESOURCE,
        &CanonicalKey::iri(iri.as_str())?,
    ))
}

/// Quads over `store` owning every bucket but `bucket`.
fn owning_all_but(store: SimStore, bucket: Bucket) -> TestResult<TestQuads> {
    let b = bucket.get();
    let mut ranges = Vec::new();
    if b > 0 {
        ranges.push((Bucket::new(0)?, Bucket::new(b - 1)?));
    }
    if b < Bucket::MAX.get() {
        ranges.push((Bucket::new(b + 1)?, Bucket::MAX));
    }
    let owned = OwnedBuckets::new(&ranges).context("disjoint ranges")?;
    Ok(QuadStore::open(lists_over(store), blocks(), RandomSource::new(7))?.owning(owned))
}

/// The bucket `e` refuses as another partition's, after checking it names `iri`.
fn not_owned(e: ultragraphdb_store_quads::QuadError, iri: &NamedNode) -> TestResult<u32> {
    let text = format!("{e:?}");
    let QuadErrorEnum::NotOwned { iri: named, bucket } = e.0 else {
        return Err(TestError::new(format!("not NotOwned: {text}")));
    };
    assert_eq!(named, iri.as_str());
    Ok(bucket)
}

#[test]
fn test_an_iri_of_another_partition_s_bucket_is_refused_in_every_position() -> TestResult {
    let foreign = ex("foreign");
    let bucket = bucket_of(&foreign)?;
    let store = owning_all_but(sim(), bucket)?;
    assert!(!store.owned().owns(bucket));
    let refusals = [
        WriteTerms::new(NS).iri(&store, foreign.as_str()).err(),
        WriteTerms::new(NS)
            .node(&store, NamedOrBlankNode::from(foreign.clone()).as_ref())
            .err(),
        WriteTerms::new(NS)
            .predicate(&store, foreign.as_ref())
            .err(),
        WriteTerms::new(NS)
            .term(&store, Term::from(foreign.clone()).as_ref())
            .err(),
        WriteTerms::new(NS)
            .graph(&store, GraphName::NamedNode(foreign.clone()).as_ref())
            .err(),
    ];
    for refused in refusals {
        let bucket_named = not_owned(refused.context("refused")?, &foreign)?;
        assert_eq!(bucket_named, bucket.get());
    }
    Ok(())
}

#[test]
fn test_an_owned_iri_is_created_beside_a_refused_one() -> TestResult {
    let foreign = ex("foreign");
    let store = owning_all_but(sim(), bucket_of(&foreign)?)?;
    let mut terms = WriteTerms::new(NS);
    let own = ex("own");
    let id = terms.iri(&store, own.as_str())?;
    assert_eq!(id.bucket(), bucket_of(&own)?);
    Ok(())
}

#[test]
fn test_a_held_iri_of_another_partition_s_bucket_resolves() -> TestResult {
    let foreign = ex("foreign");
    let store = sim();
    write(
        &quads_over(store.clone())?,
        NS,
        &[quad("a", "knows", foreign.clone())],
    )?;
    let held = quads_over(store.clone())?
        .id_of(NS, foreign.as_str())?
        .context("held")?;
    let partition = owning_all_but(store, bucket_of(&foreign)?)?;
    assert_eq!(WriteTerms::new(NS).iri(&partition, foreign.as_str())?, held);
    Ok(())
}

#[test]
fn test_a_copied_iri_resolves_to_the_id_its_home_gave() -> TestResult {
    let foreign = ex("foreign");
    let bucket = bucket_of(&foreign)?;
    let store = owning_all_but(sim(), bucket)?;
    let given = SystemId::new(TypePrefix::RESOURCE, bucket, Sequence::new(77)?);
    let mut terms = WriteTerms::new(NS);
    terms.copy(foreign.as_str(), given);
    assert_eq!(terms.iri(&store, foreign.as_str())?, given);
    assert_eq!(
        terms.written().collect::<Vec<_>>(),
        vec![(foreign.as_str(), given)],
        "the copy's entries are written by the commit"
    );
    Ok(())
}

#[test]
fn test_a_new_blank_node_takes_an_owned_bucket() -> TestResult {
    let foreign = ex("foreign");
    let bucket = bucket_of(&foreign)?;
    let store = owning_all_but(sim(), bucket)?;
    let mut terms = WriteTerms::new(NS);
    for label in ["x", "y", "z"] {
        let blank = terms.node(&store, BlankNode::new_unchecked(label).as_ref().into())?;
        assert!(store.owned().owns(blank.bucket()));
    }
    Ok(())
}

#[test]
fn test_a_store_on_its_own_owns_every_bucket() -> TestResult {
    let store = quads_over(sim())?;
    assert_eq!(store.owned(), &OwnedBuckets::all());
    Ok(())
}
