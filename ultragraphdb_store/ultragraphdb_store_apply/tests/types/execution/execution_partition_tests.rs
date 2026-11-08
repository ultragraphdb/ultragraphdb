/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What an execution does as one partition's: reverse reads behind the object's predicate list, index lookups, reads
//! that consult the subject's predicate list first (partitions, "Absence is proven by a node's predicate lists"),
//! adopted IDs of other partitions' entities, marks read and written (partitions, "A commit under a registration reply
//! reads its mark"), and the unique values it writes.

use std::num::NonZeroU64;

use oxrdf::{GraphName, Literal, NamedNodeRef, NamedOrBlankNode, Term};
use ultragraphdb_core_ids::{BucketGroup, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::{Applied, ChangesetErrorEnum, Execution, MarkedReference};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{QuadErrorEnum, id_key, term_key};
use ultragraphdb_store_sim::{SimListCache, SimStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;

use crate::utils::changesets::{
    TENANT, TestChangesets, changeset, changesets, declare, encoded, ex, exact_token, insert, term,
    uid, write,
};
use crate::utils::partitions::{bucket_of, mark_key, owning_all_but};

type TestExecution<'a> = Execution<'a, SimStore, SimListCache<CachedList>, TestBlockSource>;

fn begin(a: &TestChangesets) -> TestResult<TestExecution<'_>> {
    Ok(a.begin(TENANT, a.quads().lists().applied_ts())?)
}

fn generation(n: u64) -> TestResult<NonZeroU64> {
    NonZeroU64::new(n).context("a generation is at least 1")
}

/// The IRIs `nodes` name, as `<name>` texts, sorted.
fn names(nodes: &[NamedOrBlankNode]) -> Vec<String> {
    let mut names: Vec<String> = nodes.iter().map(ToString::to_string).collect();
    names.sort();
    names
}

fn iris(list: &[&str]) -> Vec<String> {
    let mut iris: Vec<String> = list
        .iter()
        .map(|n| format!("<{}>", ex(n).as_str()))
        .collect();
    iris.sort();
    iris
}

fn subjects(
    e: &mut TestExecution<'_>,
    predicate: &str,
    object: &str,
) -> TestResult<Vec<NamedOrBlankNode>> {
    let mut out = vec![NamedOrBlankNode::from(ex("stale"))];
    let (p, o) = (ex(predicate), term(object)?);
    e.subjects_into(
        GraphName::DefaultGraph.as_ref(),
        p.as_ref(),
        o.as_ref(),
        &mut out,
    )?;
    Ok(out)
}

fn objects(e: &mut TestExecution<'_>, subject: &str, predicate: &str) -> TestResult<Vec<Term>> {
    let mut out = vec![Term::from(ex("stale"))];
    let (s, p) = (ex(subject), ex(predicate));
    e.objects_into(
        GraphName::DefaultGraph.as_ref(),
        s.as_ref().into(),
        p.as_ref(),
        &mut out,
    )?;
    Ok(out)
}

fn reverse_key(a: &TestChangesets, predicate: &str, object: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Reverse {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate)?,
        object: EncodedTerm::id(uid(a, object)?),
    }
    .to_bytes())
}

fn object_predicates_key(a: &TestChangesets, object: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::ObjectPredicates {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        object: EncodedTerm::id(uid(a, object)?),
    }
    .to_bytes())
}

fn subject_predicates_key(a: &TestChangesets, subject: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::SubjectPredicates {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        subject: uid(a, subject)?,
    }
    .to_bytes())
}

fn forward_key(a: &TestChangesets, subject: &str, predicate: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate)?,
        subject: uid(a, subject)?,
    }
    .to_bytes())
}

#[test]
fn test_subjects_read_the_reverse_list_behind_the_object_s_predicate_list() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>", "<c> <knows> <b>"])?;
    let mut e = begin(&a)?;
    e.apply(&changeset(vec![insert("<d> <knows> <b>")?]))?;
    assert_eq!(
        names(&subjects(&mut e, "knows", "<b>")?),
        iris(&["a", "c", "d"])
    );
    let executed = e.finish()?;
    let lists = executed.reads().lists();
    assert!(
        lists.contains(&object_predicates_key(&a, "b")?),
        "the predicate list is read"
    );
    assert!(
        lists.contains(&reverse_key(&a, "knows", "b")?),
        "the reverse list is read whole"
    );
    Ok(())
}

#[test]
fn test_a_reverse_read_of_an_object_with_no_object_predicates_entry_reads_no_reverse_list()
-> TestResult {
    let a = changesets()?;
    write(&a, &["<x> <likes> <b>", "<y> <knows> <z>"])?;
    let mut e = begin(&a)?;
    assert!(subjects(&mut e, "knows", "<b>")?.is_empty());
    let executed = e.finish()?;
    let lists = executed.reads().lists();
    assert!(lists.contains(&object_predicates_key(&a, "b")?));
    assert!(
        !lists.contains(&reverse_key(&a, "knows", "b")?),
        "knows is not b's"
    );
    Ok(())
}

#[test]
fn test_a_reverse_read_of_an_object_the_execution_created_reads_no_list() -> TestResult {
    let a = changesets()?;
    write(&a, &["<x> <knows> <y>"])?;
    let mut e = begin(&a)?;
    e.apply(&changeset(vec![insert("<s> <knows> <fresh>")?]))?;
    assert_eq!(names(&subjects(&mut e, "knows", "<fresh>")?), iris(&["s"]));
    let executed = e.finish()?;
    let fresh = executed
        .terms()
        .written()
        .find(|(t, _)| *t == ex("fresh").as_str())
        .map(|(_, id)| id)
        .context("created")?;
    let op = QuadKey::ObjectPredicates {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        object: EncodedTerm::id(fresh),
    }
    .to_bytes();
    assert!(!executed.reads().lists().contains(&op));
    Ok(())
}

#[test]
fn test_a_reverse_read_of_a_literal_reads_its_holders() -> TestResult {
    let a = changesets()?;
    write(
        &a,
        &[
            "<a> <name> \"Ann\"",
            "<b> <name> \"Ann\"",
            "<c> <name> \"Bob\"",
        ],
    )?;
    let mut e = begin(&a)?;
    assert_eq!(
        names(&subjects(&mut e, "name", "\"Ann\"")?),
        iris(&["a", "b"])
    );
    // A literal no statement holds, and a blank node the execution did not name.
    assert!(subjects(&mut e, "name", "\"Eve\"")?.is_empty());
    assert!(subjects(&mut e, "knows", "_:nobody")?.is_empty());
    Ok(())
}

/// The subjects `<predicate>`'s `tokenizer` index holds for `value`, as the execution looks them up.
fn lookup(
    e: &mut TestExecution<'_>,
    predicate: &str,
    tokenizer: &str,
    value: &str,
) -> Result<Vec<NamedOrBlankNode>, ultragraphdb_store_apply::ChangesetError> {
    let mut out = vec![NamedOrBlankNode::from(ex("stale"))];
    let p = ex(predicate);
    let v = term(value).map_err(|_| ultragraphdb_store_apply::ChangesetError::not_unique(""))?;
    e.lookup_into(
        GraphName::DefaultGraph.as_ref(),
        p.as_ref(),
        tokenizer,
        v.as_ref(),
        &mut out,
    )?;
    Ok(out)
}

#[test]
fn test_a_lookup_reads_the_index_keys_of_the_value_s_tokens_sorted_by_id() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with an `exact` index")?;
    write(
        &a,
        &[
            "<a> <name> \"Ann\"",
            "<b> <name> \"Ann\"",
            "<c> <name> \"Bob\"",
        ],
    )?;
    let mut e = begin(&a)?;
    e.apply(&changeset(vec![insert("<d> <name> \"Ann\"")?]))?;
    let found = lookup(&mut e, "name", "exact", "\"Ann\"")?;
    assert_eq!(names(&found), iris(&["a", "b", "d"]));
    // The held subjects come in ID order.
    let d = format!("<{}>", ex("d").as_str());
    let held: Vec<SystemId> = found
        .iter()
        .filter(|n| n.to_string() != d)
        .map(|n| {
            uid(
                &a,
                n.to_string()
                    .trim_start_matches("<http://ex.org/")
                    .trim_end_matches('>'),
            )
        })
        .collect::<TestResult<_>>()?;
    let mut by_id = vec![uid(&a, "a")?, uid(&a, "b")?];
    by_id.sort_by_key(|i| i.to_u128());
    assert_eq!(held, by_id);
    assert!(lookup(&mut e, "name", "exact", "\"Eve\"")?.is_empty());
    let executed = e.finish()?;
    let (tokenizer, token) = exact_token("Ann");
    let key = QuadKey::ValueIndex {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, "name")?,
        tokenizer,
        token,
    }
    .to_bytes();
    assert!(
        executed.reads().lists().contains(&key),
        "the index key is read"
    );
    Ok(())
}

#[test]
fn test_a_lookup_by_an_index_the_catalog_does_not_declare_is_refused() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with an `exact` index")?;
    write(&a, &["<a> <name> \"Ann\"", "<a> <age> 30"])?;
    let mut e = begin(&a)?;
    for (predicate, tokenizer) in [("name", "term"), ("age", "int"), ("never", "exact")] {
        let refused = lookup(&mut e, predicate, tokenizer, "\"Ann\"")
            .err()
            .context("refused")?;
        assert_eq!(
            refused.0,
            ChangesetErrorEnum::NoIndex {
                predicate: ex(predicate).as_str().to_string(),
                tokenizer: tokenizer.to_string()
            }
        );
    }
    Ok(())
}

#[test]
fn test_a_read_consults_the_subject_s_predicate_list_first() -> TestResult {
    // partitions, "A predicate the node lacks is not read".
    let a = changesets()?;
    write(&a, &["<a> <name> \"Ann\"", "<b> <age> 30"])?;
    let mut e = begin(&a)?;
    assert!(objects(&mut e, "a", "age")?.is_empty());
    assert_eq!(
        objects(&mut e, "a", "name")?,
        vec![Term::from(Literal::new_simple_literal("Ann"))]
    );
    let executed = e.finish()?;
    let lists = executed.reads().lists();
    assert!(lists.contains(&subject_predicates_key(&a, "a")?));
    assert!(
        !lists.contains(&forward_key(&a, "a", "age")?),
        "no age list is read"
    );
    assert!(lists.contains(&forward_key(&a, "a", "name")?));
    Ok(())
}

#[test]
fn test_a_predicate_the_execution_gave_a_held_node_is_read_from_its_own_writes() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <name> \"Ann\"", "<b> <age> 30"])?;
    let mut e = begin(&a)?;
    e.apply(&changeset(vec![insert("<a> <age> 41")?]))?;
    assert_eq!(objects(&mut e, "a", "age")?, vec![term("41")?]);
    Ok(())
}

#[test]
fn test_a_node_created_in_the_execution_reads_no_list() -> TestResult {
    let a = changesets()?;
    write(&a, &["<b> <name> \"Bob\""])?;
    let mut e = begin(&a)?;
    e.apply(&changeset(vec![insert("<n> <name> \"x\"")?]))?;
    assert_eq!(
        objects(&mut e, "n", "name")?,
        vec![Term::from(Literal::new_simple_literal("x"))]
    );
    let executed = e.finish()?;
    let n = executed
        .terms()
        .written()
        .find(|(t, _)| *t == ex("n").as_str())
        .map(|(_, id)| id)
        .context("created")?;
    let sp = QuadKey::SubjectPredicates {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        subject: n,
    }
    .to_bytes();
    assert!(!executed.reads().lists().contains(&sp));
    assert!(
        executed
            .reads()
            .lists()
            .iter()
            .all(|k| !k.ends_with(&n.to_be_bytes())),
        "no list of n is read"
    );
    Ok(())
}

#[test]
fn test_an_adopted_iri_is_written_by_the_id_its_home_gave_and_its_entries_copied() -> TestResult {
    let a = owning_all_but("foreign")?;
    write(&a, &["<a> <name> \"Ann\""])?;
    let given = SystemId::new(
        TypePrefix::RESOURCE,
        bucket_of("foreign")?,
        Sequence::new(77)?,
    );
    let mut refused = begin(&a)?;
    let error = refused
        .apply(&changeset(vec![insert("<a> <knows> <foreign>")?]))
        .err()
        .context("not owned")?;
    assert!(
        matches!(&error.0, ChangesetErrorEnum::Quad(q) if matches!(&q.0, QuadErrorEnum::NotOwned { .. })),
        "{error:?}"
    );
    a.release_execution(refused);
    let mut e = begin(&a)?;
    e.adopt(ex("foreign").as_str(), given);
    e.apply(&changeset(vec![insert("<a> <knows> <foreign>")?]))?;
    let executed = e.finish()?;
    let versionless: Vec<(Vec<u8>, Option<Vec<u8>>)> = executed
        .writes()
        .versionless()
        .iter()
        .map(|v| (v.key().to_vec(), v.value().map(<[u8]>::to_vec)))
        .collect();
    assert!(versionless.contains(&(
        term_key(TENANT, ex("foreign").as_str()),
        Some(given.to_be_bytes().to_vec())
    )));
    assert!(versionless.contains(&(
        id_key(TENANT, given),
        Some(ex("foreign").as_str().as_bytes().to_vec())
    )));
    let version = a.quads().lists().next_version();
    let _applied: Vec<Applied> = a.apply_round(vec![(version, executed)])?;
    assert_eq!(
        a.quads().id_of(TENANT, ex("foreign").as_str())?,
        Some(given)
    );
    Ok(())
}

#[test]
fn test_a_mark_is_read_into_the_read_set_written_at_commit_and_listed() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <name> \"Ann\""])?;
    let entity = SystemId::new(TypePrefix::RESOURCE, bucket_of("o")?, Sequence::new(5)?);
    let group = BucketGroup::new(7)?;
    let mut e = begin(&a)?;
    assert_eq!(e.read_mark(entity, group)?, None, "a mark absent");
    e.mark(entity, group, generation(4)?)?;
    assert_eq!(
        e.read_mark(entity, group)?,
        Some(generation(4)?),
        "its own mark"
    );
    // Marked again at a later reply's generation: one mark, the later one.
    e.mark(entity, group, generation(5)?)?;
    let executed = e.finish()?;
    assert_eq!(
        executed.marks(),
        &[MarkedReference::new(entity, group, generation(5)?)]
    );
    assert!(executed.reads().lists().contains(&mark_key(entity, group)));
    let version = a.quads().lists().next_version();
    let _applied: Vec<Applied> = a.apply_round(vec![(version, executed)])?;
    assert_eq!(
        a.quads().mark(TENANT, entity, group, version)?,
        Some(generation(5)?)
    );
    assert_eq!(a.quads().mark(TENANT, entity, group, version - 1)?, None);
    let mut later = begin(&a)?;
    assert_eq!(
        later.read_mark(entity, group)?,
        Some(generation(5)?),
        "a mark present"
    );
    assert_eq!(
        later.read_mark(entity, BucketGroup::new(9)?)?,
        None,
        "another group's"
    );
    let read = later.finish()?;
    assert!(read.reads().lists().contains(&mark_key(entity, group)));
    assert!(read.marks().is_empty(), "a read marks nothing");
    Ok(())
}

#[test]
fn test_a_renewal_committed_after_a_reply_writes_the_key_the_reply_s_commit_read() -> TestResult {
    // partitions, "A renewal between the reply and the commit conflicts": T read (o, 7) under its reply; the renewal's
    // commit writes that key.
    let a = changesets()?;
    write(&a, &["<a> <name> \"Ann\""])?;
    let entity = SystemId::new(TypePrefix::RESOURCE, bucket_of("o")?, Sequence::new(5)?);
    let group = BucketGroup::new(7)?;
    let mut t = begin(&a)?;
    t.mark(entity, group, generation(4)?)?;
    let mut renewal = begin(&a)?;
    renewal.mark(entity, group, generation(5)?)?;
    let renewed = renewal.finish()?;
    assert!(
        renewed
            .writes()
            .entry_keys()
            .any(|k| k == mark_key(entity, group).as_slice())
    );
    let t = t.finish()?;
    assert!(t.reads().lists().contains(&mark_key(entity, group)));
    Ok(())
}

#[test]
fn test_unique_writes_name_each_unique_value_written_with_its_predicate() -> TestResult {
    let a = changesets()?;
    declare(&a, "id", "unique")?;
    write(&a, &["<z> <id> \"held\""])?;
    let mut e = begin(&a)?;
    assert!(e.unique_writes().is_empty());
    e.apply(&changeset(vec![
        insert("<a> <id> \"x\"")?,
        insert("<a> <name> \"not unique\"")?,
        insert("<b> <id> \"y\"")?,
    ]))?;
    let id = uid(&a, "id")?;
    let expected = vec![
        (id, encoded(&term("\"x\"")?)?.as_bytes().to_vec()),
        (id, encoded(&term("\"y\"")?)?.as_bytes().to_vec()),
    ];
    assert_eq!(e.unique_writes(), expected.as_slice());
    Ok(())
}

#[test]
fn test_a_read_of_an_iri_another_partition_owns_names_it_unknown() -> TestResult {
    // A predicate of another partition's bucket the partition holds no ID for is refused, as every resolution is.
    let a = owning_all_but("foreign")?;
    write(&a, &["<a> <name> \"Ann\""])?;
    let mut e = begin(&a)?;
    let mut out = Vec::new();
    let p = ex("foreign");
    let refused = e
        .objects_into(
            GraphName::DefaultGraph.as_ref(),
            ex("a").as_ref().into(),
            NamedNodeRef::from(&p),
            &mut out,
        )
        .err()
        .context("refused")?;
    let expected = bucket_of("foreign")?.get();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Quad(q)
            if matches!(&q.0, QuadErrorEnum::NotOwned { iri, bucket } if iri == p.as_str() && *bucket == expected)),
        "{refused:?}"
    );
    Ok(())
}

/// Marks of one entity under two groups, and of one group for two entities, are three marks: each read and each renewal
/// finds its own.
#[test]
fn test_marks_sharing_an_entity_or_a_group_stay_apart() -> TestResult {
    let a = changesets()?;
    let one = SystemId::new(TypePrefix::RESOURCE, bucket_of("o")?, Sequence::new(5)?);
    let two = SystemId::new(TypePrefix::RESOURCE, bucket_of("o")?, Sequence::new(6)?);
    let (seven, nine) = (BucketGroup::new(7)?, BucketGroup::new(9)?);
    let mut e = begin(&a)?;
    e.mark(one, seven, generation(1)?)?;
    e.mark(one, nine, generation(2)?)?;
    e.mark(two, seven, generation(3)?)?;
    assert_eq!(e.read_mark(one, seven)?, Some(generation(1)?));
    assert_eq!(e.read_mark(one, nine)?, Some(generation(2)?));
    assert_eq!(e.read_mark(two, seven)?, Some(generation(3)?));
    assert_eq!(e.read_mark(two, nine)?, None, "no mark of the pair");
    e.mark(one, nine, generation(4)?)?;
    let executed = e.finish()?;
    assert_eq!(
        executed.marks(),
        &[
            MarkedReference::new(one, seven, generation(1)?),
            MarkedReference::new(one, nine, generation(4)?),
            MarkedReference::new(two, seven, generation(3)?),
        ]
    );
    Ok(())
}

/// A unique value written twice is one value for its bucket derivation, and the same value under another unique
/// predicate is another.
#[test]
fn test_a_unique_value_written_twice_is_named_once() -> TestResult {
    let a = changesets()?;
    declare(&a, "id", "unique")?;
    declare(&a, "code", "unique")?;
    write(&a, &["<z> <id> \"held\""])?;
    write(&a, &["<z> <code> \"held\""])?;
    let mut e = begin(&a)?;
    e.apply(&changeset(vec![insert("<a> <id> \"x\"")?]))?;
    e.apply(&changeset(vec![insert("<a> <id> \"x\"")?]))?;
    e.apply(&changeset(vec![insert("<b> <code> \"x\"")?]))?;
    let value = encoded(&term("\"x\"")?)?.as_bytes().to_vec();
    let expected = vec![(uid(&a, "id")?, value.clone()), (uid(&a, "code")?, value)];
    assert_eq!(e.unique_writes(), expected.as_slice());
    Ok(())
}

#[test]
fn test_a_node_s_id_is_the_store_s_or_a_new_one_kept_for_the_execution() -> TestResult {
    let a = changesets()?;
    write(&a, &["<b> <name> \"Bob\""])?;
    let mut e = begin(&a)?;
    let (b, n) = (ex("b"), ex("n"));
    assert_eq!(e.node_id(b.as_ref().into())?, uid(&a, "b")?);
    let first = e.node_id(n.as_ref().into())?;
    assert_ne!(first, uid(&a, "b")?);
    assert_eq!(e.node_id(n.as_ref().into())?, first);
    Ok(())
}

/// A read names nothing where its graph or its predicate is unknown, or where its value is no literal an index holds;
/// a value of several tokens finds the subjects every token's key holds.
#[test]
fn test_reads_of_what_the_store_does_not_hold_find_nothing() -> TestResult {
    let a = changesets()?;
    declare(&a, "title", "with a `term` index")?;
    write(
        &a,
        &[
            "<a> <title> \"red fox\"",
            "<b> <title> \"red hen\"",
            "<a> <knows> <b>",
        ],
    )?;
    let mut e = begin(&a)?;
    let nowhere = GraphName::NamedNode(ex("nowhere"));
    let (knows, title, b) = (ex("knows"), ex("title"), term("<b>")?);
    let mut out = vec![NamedOrBlankNode::from(ex("stale"))];
    e.subjects_into(nowhere.as_ref(), knows.as_ref(), b.as_ref(), &mut out)?;
    assert!(out.is_empty(), "an unknown graph");
    assert!(
        subjects(&mut e, "never", "<b>")?.is_empty(),
        "an unknown predicate"
    );
    e.lookup_into(
        nowhere.as_ref(),
        title.as_ref(),
        "term",
        term("\"red\"")?.as_ref(),
        &mut out,
    )?;
    assert!(out.is_empty(), "an unknown graph");
    assert!(
        lookup(&mut e, "title", "term", "<b>")?.is_empty(),
        "no literal"
    );
    assert_eq!(
        names(&lookup(&mut e, "title", "term", "\"red\"")?),
        iris(&["a", "b"])
    );
    assert_eq!(
        names(&lookup(&mut e, "title", "term", "\"red fox\"")?),
        iris(&["a"])
    );
    Ok(())
}
