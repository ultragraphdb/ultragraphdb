/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{BlankNode, GraphNameRef, Literal, NamedNode, Term};
use ultragraphdb_core_ids::{Bucket, SystemId, TypePrefix};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_store_quads::{
    OwnedBuckets, QuadErrorEnum, QuadStore, SKOLEM_DRAWS_MAX, WriteTerms, id_key, literal_key,
    tenant_base, term_key,
};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::VersionlessEntry;

use crate::utils::helpers::{
    TestQuads, blocks, ex, lists_over, quad, quads, quads_with, sim, write,
};

/// The bucket of `http://ex.org/a` in tenant 0: core_ids' `testdata/golden/buckets.json`, "the scenario: tenant
/// 0", from the xxHash reference.
const BUCKET_A_TENANT0: u32 = 0xC7_F1AB;
/// The first bucket of tenant 0's run, from the same file.
const RUN_0: u32 = 0xC7_0000;

fn iri(terms: &mut WriteTerms, store: &TestQuads, name: &str) -> TestResult<SystemId> {
    Ok(terms.iri(store, ex(name).as_str())?)
}

fn skolem_prefix(tenant: u64) -> String {
    format!("{}.well-known/genid/", tenant_base(tenant))
}

/// The bucket a skolem IRI of tenant `tenant` states, read by hand from its text.
fn stated_bucket(tenant: u64, skolem: &str) -> TestResult<u32> {
    let tail = skolem
        .strip_prefix(&skolem_prefix(tenant))
        .context("under the skolem base")?;
    let (bucket, random) = tail.split_once('-').context("a bucket and random digits")?;
    assert_eq!(bucket.len(), 6, "{skolem}");
    assert_eq!(random.len(), 26, "{skolem}");
    assert!(
        tail.bytes()
            .all(|b| b == b'-' || b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{skolem}"
    );
    Ok(u32::from_str_radix(bucket, 16)?)
}

/// The skolem IRI of the blank node `id` this write minted.
fn skolem_of(terms: &WriteTerms, id: SystemId) -> TestResult<String> {
    let (text, skolem) = terms.pending_iri(id).context("the write minted it")?;
    assert!(skolem, "a blank node is skolemized");
    Ok(text.to_string())
}

#[test]
fn test_a_new_iri_takes_its_computed_bucket_the_resource_type_and_a_leased_sequence() -> TestResult
{
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    let a = iri(&mut terms, &store, "a")?;
    assert_eq!(a.type_prefix(), TypePrefix::RESOURCE);
    assert_eq!(a.bucket().get(), BUCKET_A_TENANT0);
    // The test source's first block of the Resource type starts at 1.
    assert_eq!(a.sequence().get(), 1);
    assert!(terms.is_fresh(a));
    assert_eq!(terms.absent().collect::<Vec<_>>(), vec![a]);
    assert_eq!(
        iri(&mut terms, &store, "a")?,
        a,
        "one ID per IRI in a write"
    );
    assert_eq!(iri(&mut terms, &store, "b")?.sequence().get(), 2);
    Ok(())
}

#[test]
fn test_concurrent_writes_of_one_new_iri_share_its_id() -> TestResult {
    let store = quads()?;
    let mut first = WriteTerms::new(0);
    let mut second = WriteTerms::new(0);
    let created = iri(&mut first, &store, "new")?;
    let taken = iri(&mut second, &store, "new")?;
    assert_eq!(created, taken);
    assert!(
        first.is_fresh(created),
        "the creator's snapshot holds no list of it"
    );
    assert!(
        !second.is_fresh(taken),
        "a commit since the second write's snapshot may hold it"
    );
    assert_eq!(first.absent().collect::<Vec<_>>(), vec![created]);
    assert_eq!(
        second.absent().count(),
        0,
        "the second write did not create it"
    );
    assert_eq!(
        first.written().collect::<Vec<_>>(),
        vec![(ex("new").as_str(), created)]
    );
    assert_eq!(
        second.written().collect::<Vec<_>>(),
        vec![(ex("new").as_str(), created)]
    );
    Ok(())
}

#[test]
fn test_a_held_iri_is_neither_fresh_nor_absent_nor_written_again() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("known", "p", ex("other"))])?;
    let held = store.id_of(0, ex("known").as_str())?.context("committed")?;
    let mut terms = WriteTerms::new(0);
    assert_eq!(iri(&mut terms, &store, "known")?, held);
    assert!(!terms.is_fresh(held));
    assert_eq!(terms.absent().count(), 0);
    assert_eq!(terms.written().count(), 0);
    assert!(terms.entries().is_empty());
    Ok(())
}

#[test]
fn test_absent_ids_are_recorded_once_in_id_order() -> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(3);
    let zeta = iri(&mut terms, &store, "zeta")?;
    let alpha = iri(&mut terms, &store, "alpha")?;
    let _zeta_again: SystemId = iri(&mut terms, &store, "zeta")?;
    let found = terms
        .find(&store, Term::from(ex("beta")).as_ref())?
        .context("an IRI is found or created")?;
    let beta = found.as_id().context("an ID")?;
    let mut expected = vec![zeta, alpha, beta];
    expected.sort();
    assert_eq!(terms.absent().collect::<Vec<_>>(), expected);
    Ok(())
}

#[test]
fn test_a_blank_node_is_one_new_id_per_label_named_by_a_skolem_iri() -> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    let x = terms.blank(&store, "x")?;
    let y = terms.blank(&store, "y")?;
    assert_eq!(terms.blank(&store, "x")?, x);
    assert_ne!(x, y);
    assert_eq!(x.type_prefix(), TypePrefix::RESOURCE);
    let skolem = skolem_of(&terms, x)?;
    assert_eq!(
        stated_bucket(0, &skolem)?,
        x.bucket().get(),
        "its bucket is the one its skolem IRI states"
    );
    assert_eq!(
        x.bucket().get() >> 16,
        RUN_0 >> 16,
        "a bucket of tenant 0's run"
    );
    assert!(terms.is_fresh(x));
    assert_eq!(terms.absent().count(), 0, "a blank node is never looked up");
    assert!(terms.written().any(|(t, id)| t == skolem && id == x));
    Ok(())
}

#[test]
fn test_a_blank_nodes_skolem_iri_names_its_bucket_and_nothing_else() -> TestResult {
    // The term-dictionary scenario's partition 1, owning the first 21,846 buckets of tenant 0's run, its random
    // source seeded 0. SplitMix64 seeded 0 gives 0xe220a8397b1dcdaf, 0x6e789e6aa1b965f4, 0x06c45d188009454f (the
    // reference splitmix64.c): the bucket is the first reduced below 21,846, 19,296 above the run's start (Lemire's
    // multiply-shift, by hand in Python), and the 104 random bits the second's low 40 bits above the third.
    let owned = OwnedBuckets::new(&[(Bucket::new(RUN_0)?, Bucket::new(RUN_0 + 21_845)?)])
        .context("ranges")?;
    let mint = || -> TestResult<(SystemId, String)> {
        let store = quads_with(sim(), blocks(), 0)?.owning(owned.clone());
        let mut terms = WriteTerms::new(0);
        let x = terms.blank(&store, "x")?;
        Ok((x, skolem_of(&terms, x)?))
    };
    let (x, skolem) = mint()?;
    assert_eq!(
        skolem,
        format!("{}c74b60-6aa1b965f406c45d188009454f", skolem_prefix(0))
    );
    assert_eq!(x.bucket().get(), 0xC7_4B60);
    assert_eq!(mint()?.1, skolem, "the same seed mints the same IRI");
    Ok(())
}

#[test]
fn test_a_skolem_iri_the_dictionary_holds_is_drawn_again() -> TestResult {
    let shared = sim();
    let source = blocks();
    let first = quads_with(shared.clone(), source.clone(), 21)?;
    let mut terms = WriteTerms::new(0);
    let x = terms.blank(&first, "x")?;
    let held = skolem_of(&terms, x)?;
    write_terms(&first, terms)?;
    // A store drawing from the same seed draws the held IRI first.
    let again = quads_with(shared, source, 21)?;
    let mut redraw = WriteTerms::new(0);
    let y = redraw.blank(&again, "y")?;
    let skolem = skolem_of(&redraw, y)?;
    assert_ne!(skolem, held);
    assert_ne!(y, x);
    assert_eq!(stated_bucket(0, &skolem)?, y.bucket().get());
    Ok(())
}

#[test]
fn test_draws_naming_held_iris_past_the_bound_are_refused() -> TestResult {
    // Stores over one seed each mint one blank node and commit it: the store after them draws the held IRIs first, so
    // the store after SKOLEM_DRAWS_MAX of them is refused.
    let shared = sim();
    let source = blocks();
    for _ in 0..SKOLEM_DRAWS_MAX {
        let store = quads_with(shared.clone(), source.clone(), 5)?;
        let mut terms = WriteTerms::new(0);
        let _x: SystemId = terms.blank(&store, "x")?;
        write_terms(&store, terms)?;
    }
    let store = quads_with(shared, source, 5)?;
    let error = WriteTerms::new(0)
        .blank(&store, "x")
        .err()
        .context("refused")?;
    assert_eq!(
        error.0,
        QuadErrorEnum::SkolemDraws {
            draws: SKOLEM_DRAWS_MAX
        }
    );
    Ok(())
}

#[test]
fn test_a_blank_node_is_refused_where_no_bucket_of_the_run_is_owned() -> TestResult {
    let elsewhere =
        OwnedBuckets::new(&[(Bucket::new(0)?, Bucket::new(0xFF)?)]).context("ranges")?;
    let store = quads()?.owning(elsewhere);
    let error = WriteTerms::new(0)
        .blank(&store, "x")
        .err()
        .context("refused")?;
    assert_eq!(error.0, QuadErrorEnum::NoOwnedBucket { tenant: 0 });
    assert!(error.to_string().contains("tenant 0"), "{error}");
    Ok(())
}

#[test]
fn test_a_held_skolem_iri_names_its_blank_node_and_an_unknown_one_is_refused() -> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    let x = terms.blank(&store, "x")?;
    let skolem = skolem_of(&terms, x)?;
    write_terms(&store, terms)?;
    let mut later = WriteTerms::new(0);
    assert_eq!(later.iri(&store, &skolem)?, x);
    let forged = format!("{}abcdef-00000000000000000000000001", skolem_prefix(0));
    let error = later.iri(&store, &forged).err().context("refused")?;
    assert_eq!(
        error.0,
        QuadErrorEnum::UnknownSkolem {
            iri: forged.clone()
        }
    );
    assert!(error.to_string().contains(&forged), "{error}");
    let forged_term = Term::from(NamedNode::new_unchecked(forged));
    assert_eq!(later.find(&store, forged_term.as_ref())?, None);
    assert_eq!(later.absent().count(), 0);
    Ok(())
}

#[test]
fn test_find_names_no_blank_node_the_write_did_not_name() -> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    let x = terms.blank(&store, "x")?;
    let other = Term::from(BlankNode::new_unchecked("other"));
    assert_eq!(terms.find(&store, other.as_ref())?, None);
    let named = Term::from(BlankNode::new_unchecked("x"));
    assert_eq!(
        terms.find(&store, named.as_ref())?,
        Some(EncodedTerm::id(x))
    );
    Ok(())
}

#[test]
fn test_a_literals_datatype_is_resolved_and_its_strings_kept() -> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    let typed = Literal::new_typed_literal("a lexical form past fifteen", ex("custom"));
    let encoded = terms.term(&store, Term::from(typed).as_ref())?;
    let custom = store_iri(&terms, &ex("custom"))?;
    assert_eq!(terms.absent().collect::<Vec<_>>(), vec![custom]);
    assert!(encoded.is_literal());
    let hash = Hash128::of(b"a lexical form past fifteen");
    assert_eq!(
        terms.pending_string(&hash),
        Some("a lexical form past fifteen")
    );
    assert!(terms.entries().contains(&(
        literal_key(0, &hash),
        b"a lexical form past fifteen".to_vec()
    )));
    let found = terms.find(
        &store,
        Term::from(Literal::new_typed_literal("1", ex("custom"))).as_ref(),
    )?;
    assert!(found.is_some_and(|f| f.is_literal()));
    Ok(())
}

#[test]
fn test_a_graph_is_the_default_or_named_by_its_id() -> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    assert_eq!(
        terms.graph(&store, GraphNameRef::DefaultGraph)?,
        Graph::Default
    );
    let g = terms.graph(&store, GraphNameRef::NamedNode(ex("g").as_ref()))?;
    assert_eq!(g, Graph::Named(store_iri(&terms, &ex("g"))?));
    let b = terms.graph(
        &store,
        GraphNameRef::BlankNode(BlankNode::new_unchecked("g").as_ref()),
    )?;
    assert!(matches!(b, Graph::Named(id) if terms.is_fresh(id)));
    Ok(())
}

#[test]
fn test_a_triple_term_is_no_object() -> TestResult {
    let store = quads()?;
    let triple = oxrdf::Triple::new(ex("s"), ex("p"), ex("o"));
    let error = WriteTerms::new(0)
        .term(&store, Term::from(triple.clone()).as_ref())
        .err()
        .context("refused")?;
    assert!(matches!(error.0, QuadErrorEnum::Mapping(_)), "{error}");
    let found = WriteTerms::new(0)
        .find(&store, Term::from(triple).as_ref())
        .err()
        .context("refused")?;
    assert!(matches!(found.0, QuadErrorEnum::Mapping(_)), "{found}");
    Ok(())
}

#[test]
fn test_a_statement_id_is_of_the_statement_type_in_its_subjects_bucket_named_by_a_reifier()
-> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    let a = iri(&mut terms, &store, "a")?;
    let statement = terms.statement(&store, a)?;
    assert_eq!(statement.type_prefix(), TypePrefix::STATEMENT);
    assert_eq!(statement.bucket(), a.bucket());
    assert_eq!(
        statement.sequence().get(),
        1,
        "the Statement type's first sequence"
    );
    assert!(terms.is_fresh(statement));
    let (reifier, skolem) = terms.pending_iri(statement).context("its reifier")?;
    assert!(!skolem, "a reifier is no blank node");
    assert_eq!(stated_bucket(0, reifier)?, a.bucket().get());
    let reifier = reifier.to_string();
    assert_eq!(
        terms.reifier(&store, &reifier)?,
        Some(statement),
        "the write's own reifier"
    );
    write_terms(&store, terms)?;
    let mut later = WriteTerms::new(0);
    assert_eq!(
        later.reifier(&store, &reifier)?,
        Some(statement),
        "the dictionary's reifier"
    );
    assert_eq!(store.iri_of(0, statement)?, (reifier, false));
    Ok(())
}

#[test]
fn test_a_reifier_names_no_statement_unless_it_names_a_statement_id() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "p", ex("b"))])?;
    let mut terms = WriteTerms::new(0);
    assert_eq!(
        terms.reifier(&store, ex("a").as_str())?,
        None,
        "a resource's IRI"
    );
    assert_eq!(
        terms.reifier(&store, ex("nothing").as_str())?,
        None,
        "an IRI nothing names"
    );
    assert_eq!(
        store.id_of(0, ex("nothing").as_str())?,
        None,
        "looking a reifier up creates nothing"
    );
    assert!(terms.written().next().is_none());
    assert_eq!(terms.absent().count(), 0);
    Ok(())
}

#[test]
fn test_entries_are_both_directions_of_each_term_written_in_the_dictionary_layout() -> TestResult {
    let store = quads()?;
    let mut terms = WriteTerms::new(0);
    let a = iri(&mut terms, &store, "a")?;
    let entries = terms.entries();
    // SHA-256 of "http://ex.org/a", its first 16 bytes, by Python's hashlib: a8b865475553f3087633d7543a6227a0.
    let mut term = vec![0x1B, 0, 0, 0, 0, 0, 0, 0, 0, 0x01];
    term.extend_from_slice(&[
        0xa8, 0xb8, 0x65, 0x47, 0x55, 0x53, 0xf3, 0x08, 0x76, 0x33, 0xd7, 0x54, 0x3a, 0x62, 0x27,
        0xa0,
    ]);
    // The ID (type 1, sequence 1, bucket 0xC7F1AB) as its 16 bytes, written from the layout.
    let id_bytes = [0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0xC7, 0xF1, 0xAB, 0, 0, 0];
    assert_eq!(a.to_be_bytes(), id_bytes);
    let mut by_id = vec![0x1B, 0, 0, 0, 0, 0, 0, 0, 0, 0x02];
    by_id.extend_from_slice(&id_bytes);
    assert_eq!(
        entries,
        vec![
            (term.clone(), id_bytes.to_vec()),
            (by_id.clone(), ex("a").as_str().as_bytes().to_vec())
        ]
    );
    assert_eq!(term_key(0, ex("a").as_str()), term);
    assert_eq!(id_key(0, a), by_id);
    Ok(())
}

#[test]
fn test_inherited_entries_are_read_as_the_writes_own_and_never_written() -> TestResult {
    let store = quads()?;
    let r = |n| -> TestResult<SystemId> {
        Ok(SystemId::new(
            TypePrefix::RESOURCE,
            Bucket::new(3)?,
            ultragraphdb_core_ids::Sequence::new(n)?,
        ))
    };
    let hash = Hash128::of(b"a long string past fifteen bytes");
    let fresh = r(41)?;
    let blank = r(42)?;
    let elsewhere = r(43)?;
    let removed = r(44)?;
    let skolem = format!("{}000003-00000000000000000000000001", skolem_prefix(0));
    let entries = [
        VersionlessEntry::put(id_key(0, fresh), ex("fresh").as_str().as_bytes().to_vec()),
        VersionlessEntry::put(id_key(0, blank), skolem.as_bytes().to_vec()),
        VersionlessEntry::put(
            literal_key(0, &hash),
            b"a long string past fifteen bytes".to_vec(),
        ),
        VersionlessEntry::put(id_key(1, elsewhere), b"http://ex.org/elsewhere".to_vec()),
        VersionlessEntry::put(
            term_key(0, "http://ex.org/term"),
            elsewhere.to_be_bytes().to_vec(),
        ),
        VersionlessEntry::removal(id_key(0, removed)),
        VersionlessEntry::put(b"not a dictionary key".to_vec(), Vec::new()),
        VersionlessEntry::put(id_key(0, removed), vec![0xff]),
    ];
    let mut terms = WriteTerms::new(0);
    terms.inherit(&entries.iter().collect::<Vec<_>>());
    assert_eq!(
        terms.pending_iri(fresh),
        Some((ex("fresh").as_str(), false))
    );
    assert_eq!(terms.pending_iri(blank), Some((skolem.as_str(), true)));
    assert_eq!(
        terms.pending_string(&hash),
        Some("a long string past fifteen bytes")
    );
    assert_eq!(
        terms.pending_iri(elsewhere),
        None,
        "another tenant's entry, and a term entry"
    );
    assert_eq!(terms.pending_iri(removed), None, "a removal");
    assert_eq!(
        iri(&mut terms, &store, "fresh")?,
        fresh,
        "known by its ID, not created again"
    );
    assert_eq!(
        terms.iri(&store, &skolem)?,
        blank,
        "an inherited skolem IRI is known"
    );
    assert!(
        terms.entries().is_empty(),
        "nothing inherited is written again"
    );
    assert_eq!(terms.absent().count(), 0, "a known IRI is not looked up");
    Ok(())
}

#[test]
fn test_know_takes_an_id_without_writing_it_and_copy_writes_it() -> TestResult {
    let store = quads()?;
    let foreign = SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(0xF3_0000)?,
        ultragraphdb_core_ids::Sequence::new(9)?,
    );
    let known = SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(1)?,
        ultragraphdb_core_ids::Sequence::new(10)?,
    );
    let mut terms = WriteTerms::new(0);
    terms.copy(ex("b").as_str(), foreign);
    terms.know(ex("k").as_str(), known);
    terms.know(ex("b").as_str(), known);
    assert_eq!(
        iri(&mut terms, &store, "b")?,
        foreign,
        "what the write resolved first stays"
    );
    assert_eq!(iri(&mut terms, &store, "k")?, known);
    assert!(!terms.is_fresh(foreign));
    assert_eq!(terms.absent().count(), 0);
    assert_eq!(
        terms.entries(),
        vec![
            (
                term_key(0, ex("b").as_str()),
                foreign.to_be_bytes().to_vec()
            ),
            (id_key(0, foreign), ex("b").as_str().as_bytes().to_vec())
        ]
    );
    assert_eq!(terms.pending_iri(foreign), Some((ex("b").as_str(), false)));
    Ok(())
}

#[test]
fn test_the_pending_bound_refuses_another_new_id_until_a_commit_settles() -> TestResult {
    let store = quads()?.with_pending_max(2);
    let mut terms = WriteTerms::new(0);
    let _a: SystemId = iri(&mut terms, &store, "a")?;
    let _b: SystemId = iri(&mut terms, &store, "b")?;
    let error = WriteTerms::new(0)
        .iri(&store, ex("c").as_str())
        .err()
        .context("refused")?;
    assert_eq!(error.0, QuadErrorEnum::PendingFull { capacity: 2 });
    assert!(error.to_string().contains('2'), "{error}");
    let mut other = WriteTerms::new(0);
    assert!(
        other.iri(&store, ex("a").as_str()).is_ok(),
        "a pending ID is still taken"
    );
    write_terms(&store, terms)?;
    assert!(WriteTerms::new(0).iri(&store, ex("c").as_str()).is_ok());
    Ok(())
}

#[test]
fn test_no_sequence_refuses_a_new_iri_and_creates_nothing() -> TestResult {
    let source = blocks();
    source.set_reachable(false);
    let store = quads_with(sim(), source.clone(), 1)?;
    let error = WriteTerms::new(0)
        .iri(&store, ex("a").as_str())
        .err()
        .context("refused")?;
    assert_eq!(error.0, QuadErrorEnum::NoSequence { prefix: 1 });
    let mut statement = WriteTerms::new(0);
    source.set_reachable(true);
    let a = iri(&mut statement, &store, "a")?;
    assert_eq!(a.sequence().get(), 1, "the refusal spent nothing");
    Ok(())
}

/// The ID `iri` has among the terms the write resolved.
fn store_iri(terms: &WriteTerms, iri: &NamedNode) -> TestResult<SystemId> {
    terms
        .written()
        .find(|(t, _)| *t == iri.as_str())
        .map(|(_, id)| id)
        .context("the write resolved it")
}

/// Commits a write naming nothing but `terms`' entries.
fn write_terms(store: &TestQuads, terms: WriteTerms) -> TestResult {
    let mut batch = store.lists().batch();
    batch.push(store.lists().request(store.lists().applied_ts()));
    let _versions: Vec<u64> = store.commit(batch, &[terms])?;
    Ok(())
}

#[test]
fn test_quads_open_over_lists_with_a_seeded_source() -> TestResult {
    let opened: TestQuads = QuadStore::open(lists_over(sim()), blocks(), RandomSource::new(0))?;
    assert_eq!(opened.lists().applied_ts(), 0);
    Ok(())
}

#[test]
fn test_a_pending_skolem_iri_is_found_and_a_held_one_too() -> TestResult {
    let store = quads()?;
    let mut minting = WriteTerms::new(0);
    let x = minting.blank(&store, "x")?;
    let skolem = Term::from(NamedNode::new(skolem_of(&minting, x)?)?);
    let mut reading = WriteTerms::new(0);
    assert_eq!(
        reading.find(&store, skolem.as_ref())?,
        Some(EncodedTerm::id(x)),
        "pending"
    );
    write_terms(&store, minting)?;
    assert_eq!(
        WriteTerms::new(0).find(&store, skolem.as_ref())?,
        Some(EncodedTerm::id(x)),
        "held"
    );
    Ok(())
}

#[test]
fn test_a_found_pending_skolem_iri_is_carried_by_the_write_that_found_it() -> TestResult {
    let store = quads()?;
    let mut minting = WriteTerms::new(0);
    let x = minting.blank(&store, "x")?;
    let skolem = Term::from(NamedNode::new(skolem_of(&minting, x)?)?);
    let mut finding = WriteTerms::new(0);
    assert_eq!(
        finding.find(&store, skolem.as_ref())?,
        Some(EncodedTerm::id(x))
    );
    store.release(&minting);
    assert_eq!(
        finding.find(&store, skolem.as_ref())?,
        Some(EncodedTerm::id(x)),
        "found once, the write's own"
    );
    write_terms(&store, finding)?;
    assert!(store.iri_of(0, x)?.1, "its entries are committed");
    Ok(())
}

#[test]
fn test_statement_reifier_draws_past_the_bound_are_refused() -> TestResult {
    // As blank nodes: stores over one seed each mint one reifier of one subject and commit it.
    let shared = sim();
    let source = blocks();
    let subject = SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(0xC7_0001)?,
        ultragraphdb_core_ids::Sequence::new(1)?,
    );
    for _ in 0..SKOLEM_DRAWS_MAX {
        let store = quads_with(shared.clone(), source.clone(), 9)?;
        let mut terms = WriteTerms::new(0);
        let _statement: SystemId = terms.statement(&store, subject)?;
        write_terms(&store, terms)?;
    }
    let store = quads_with(shared, source, 9)?;
    let error = WriteTerms::new(0)
        .statement(&store, subject)
        .err()
        .context("refused")?;
    assert_eq!(
        error.0,
        QuadErrorEnum::SkolemDraws {
            draws: SKOLEM_DRAWS_MAX
        }
    );
    Ok(())
}
