/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The reads a partition answers beside its quad reads, each at a version: referrer sets and marks from values written
//! by hand, its own reverse entries behind the object's predicate list, its predicate lists, reverse counts, and the
//! read kernel over a keyspace's range.

use std::num::{NonZeroU64, NonZeroUsize};

use oxrdf::{Literal, Term};
use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_ids::{BucketGroup, Epoch, SystemId};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::DEL;
use ultragraphdb_store_quads::{KernelRow, KernelScan, QuadErrorEnum, QuadWrite, ReferrerMember};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::utils::helpers::{TestQuads, commit, ex, id, kind, quad, quads, write_at};

const NS: u64 = 0;

/// `value` written under `key` at `version`, as a commit writes an entry.
fn put(store: &TestQuads, key: Vec<u8>, version: u64, value: Vec<u8>) -> TestResult {
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(key, version, 0, value));
    store.store().apply(batch, PersistMode::Sync)?;
    Ok(())
}

fn corruption(e: ultragraphdb_store_quads::QuadError) -> TestResult<(Vec<u8>, String)> {
    let text = format!("{e:?}");
    let QuadErrorEnum::Corrupted { key, reason } = e.0 else {
        return Err(TestError::new(format!("not corruption: {text}")));
    };
    Ok((key, reason))
}

fn ids(store: &TestQuads, names: &[&str]) -> TestResult<Vec<SystemId>> {
    let mut ids = names
        .iter()
        .map(|n| id(store, NS, &ex(n)))
        .collect::<TestResult<Vec<_>>>()?;
    ids.sort_by_key(|i| i.to_u128());
    Ok(ids)
}

fn object(store: &TestQuads, name: &str) -> TestResult<EncodedTerm> {
    Ok(EncodedTerm::id(id(store, NS, &ex(name))?))
}

#[test]
fn test_referrers_read_the_set_written_at_or_below_the_version() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, NS, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, NS, &ex("o"))?;
    let key = QuadKey::Referrers {
        namespace: NS,
        entity: o,
    }
    .to_bytes();
    // Group 7 at epoch 12, generation 4, written at version 100; group 3 added at version 200.
    let seven = [
        vec![1, 0, 0, 0, 1],
        vec![0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 4],
    ]
    .concat();
    let both = [
        vec![1, 0, 0, 0, 2],
        vec![0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 1],
        vec![0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 4],
    ]
    .concat();
    put(&store, key.clone(), at + 100, seven)?;
    put(&store, key, at + 200, both)?;
    let member = |g, e, n| -> TestResult<ReferrerMember> {
        Ok(ReferrerMember::new(
            BucketGroup::new(g)?,
            Epoch::new(e)?,
            NonZeroU64::new(n).context("a generation")?,
        ))
    };
    assert!(
        store.referrers(NS, o, at + 99)?.members().is_empty(),
        "none yet"
    );
    assert_eq!(
        store.referrers(NS, o, at + 100)?.members(),
        &[member(7, 12, 4)?]
    );
    assert_eq!(
        store.referrers(NS, o, u64::MAX)?.members(),
        &[member(3, 13, 1)?, member(7, 12, 4)?]
    );
    // Another namespace's set of the same ID is another key.
    assert!(store.referrers(NS + 1, o, u64::MAX)?.members().is_empty());
    Ok(())
}

#[test]
fn test_a_referrer_set_value_that_does_not_decode_is_corruption_naming_its_key() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, NS, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, NS, &ex("o"))?;
    let key = QuadKey::Referrers {
        namespace: NS,
        entity: o,
    }
    .to_bytes();
    put(&store, key.clone(), at + 1, vec![9])?;
    let error = store.referrers(NS, o, u64::MAX).err().context("refused")?;
    assert_eq!(corruption(error)?.0, key);
    Ok(())
}

#[test]
fn test_a_mark_reads_its_generation_at_or_below_the_version() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, NS, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, NS, &ex("o"))?;
    let (seven, nine) = (BucketGroup::new(7)?, BucketGroup::new(9)?);
    let key = QuadKey::Mark {
        namespace: NS,
        entity: o,
        group: 7,
    }
    .to_bytes();
    put(&store, key.clone(), at + 10, vec![0, 0, 0, 0, 0, 0, 0, 5])?;
    put(&store, key, at + 20, vec![0, 0, 0, 0, 0, 0, 1, 0])?;
    assert_eq!(store.mark(NS, o, seven, at + 9)?, None, "absent before");
    assert_eq!(store.mark(NS, o, seven, at + 10)?, NonZeroU64::new(5));
    assert_eq!(store.mark(NS, o, seven, at + 20)?, NonZeroU64::new(256));
    assert_eq!(
        store.mark(NS, o, nine, u64::MAX)?,
        None,
        "another group's mark"
    );
    Ok(())
}

#[test]
fn test_a_mark_that_is_no_generation_of_at_least_one_is_corruption() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, NS, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, NS, &ex("o"))?;
    let group = BucketGroup::new(BucketGroup::COUNT - 1)?;
    let key = QuadKey::Mark {
        namespace: NS,
        entity: o,
        group: BucketGroup::COUNT - 1,
    }
    .to_bytes();
    put(&store, key.clone(), at + 1, vec![0; 8])?;
    let zero = store.mark(NS, o, group, at + 1).err().context("a zero")?;
    assert_eq!(corruption(zero)?.0, key);
    put(&store, key.clone(), at + 2, vec![0, 0, 0, 0, 0, 0, 1])?;
    let short = store.mark(NS, o, group, at + 2).err().context("7 bytes")?;
    assert_eq!(corruption(short)?.0, key);
    Ok(())
}

#[test]
fn test_subjects_and_counts_are_the_partition_s_own_holders_of_the_object() -> TestResult {
    let store = quads()?;
    let first = write_at(&store, NS, &[quad("a", "knows", ex("b"))])?;
    let second = write_at(
        &store,
        NS,
        &[quad("c", "knows", ex("b")), quad("c", "likes", ex("b"))],
    )?;
    let knows = id(&store, NS, &ex("knows"))?;
    let b = object(&store, "b")?;
    assert_eq!(
        store.subjects(NS, Graph::Default, knows, &b, first)?,
        ids(&store, &["a"])?
    );
    assert_eq!(
        store.subjects(NS, Graph::Default, knows, &b, second)?,
        ids(&store, &["a", "c"])?
    );
    assert_eq!(
        store.reverse_count(NS, Graph::Default, knows, &b, first)?,
        1
    );
    assert_eq!(
        store.reverse_count(NS, Graph::Default, knows, &b, second)?,
        2
    );
    // An object no subject points at with the predicate.
    let a = object(&store, "a")?;
    assert!(
        store
            .subjects(NS, Graph::Default, knows, &a, second)?
            .is_empty()
    );
    assert_eq!(
        store.reverse_count(NS, Graph::Default, knows, &a, second)?,
        0
    );
    // A literal object, which has its reverse entries too: its encoding as the forward list holds it.
    let at = write_at(
        &store,
        NS,
        &[quad("d", "name", Literal::new_simple_literal("x"))],
    )?;
    let name = id(&store, NS, &ex("name"))?;
    let (x, _) = store
        .objects(&forward_key(&store, "d", "name")?, at)?
        .into_iter()
        .next()
        .context("d holds x")?;
    assert_eq!(
        store.subjects(NS, Graph::Default, name, &x, at)?,
        ids(&store, &["d"])?
    );
    Ok(())
}

#[test]
fn test_a_reverse_list_the_object_s_predicate_list_does_not_name_is_not_read() -> TestResult {
    let store = quads()?;
    let _at: u64 = write_at(&store, NS, &[quad("a", "knows", ex("b"))])?;
    let knows = id(&store, NS, &ex("knows"))?;
    let b = object(&store, "b")?;
    // The object-predicates list loses `knows` while the reverse list keeps `a`: only the predicate list is consulted.
    let read_ts = store.lists().applied_ts();
    let mut write = QuadWrite::new(&store, NS, store.lists().request(read_ts));
    let op = QuadKey::ObjectPredicates {
        namespace: NS,
        graph: Graph::Default,
        object: b.clone(),
    }
    .to_bytes();
    write.request().get_from_delta(&op).add_posting(
        Posting::default().with_uid(knows.to_u128()).with_op(DEL),
        false,
    );
    let at = commit(&store, vec![write])?
        .first()
        .copied()
        .context("commits")?;
    let reverse = QuadKey::Reverse {
        namespace: NS,
        graph: Graph::Default,
        predicate: knows,
        object: b.clone(),
    }
    .to_bytes();
    assert_eq!(
        store.objects(&reverse, at)?.len(),
        1,
        "the reverse list still holds a"
    );
    assert!(
        store
            .object_predicates(NS, Graph::Default, &b, at)?
            .is_empty()
    );
    assert!(
        store
            .subjects(NS, Graph::Default, knows, &b, at)?
            .is_empty()
    );
    assert_eq!(store.reverse_count(NS, Graph::Default, knows, &b, at)?, 0);
    Ok(())
}

#[test]
fn test_predicate_lists_name_a_node_s_predicates_at_the_version() -> TestResult {
    let store = quads()?;
    let first = write_at(&store, NS, &[quad("a", "knows", ex("b"))])?;
    let second = write_at(
        &store,
        NS,
        &[
            quad("a", "name", Literal::new_simple_literal("x")),
            quad("c", "likes", ex("b")),
        ],
    )?;
    let a = id(&store, NS, &ex("a"))?;
    let b = object(&store, "b")?;
    assert_eq!(
        store.subject_predicates(NS, Graph::Default, a, first)?,
        ids(&store, &["knows"])?
    );
    assert_eq!(
        store.subject_predicates(NS, Graph::Default, a, second)?,
        ids(&store, &["knows", "name"])?
    );
    assert_eq!(
        store.object_predicates(NS, Graph::Default, &b, first)?,
        ids(&store, &["knows"])?
    );
    assert_eq!(
        store.object_predicates(NS, Graph::Default, &b, second)?,
        ids(&store, &["knows", "likes"])?
    );
    // A node without lists, and a version before any.
    let c = id(&store, NS, &ex("c"))?;
    assert!(
        store
            .subject_predicates(NS, Graph::Default, c, first)?
            .is_empty()
    );
    assert!(
        store
            .object_predicates(NS, Graph::Default, &EncodedTerm::id(a), second)?
            .is_empty()
    );
    Ok(())
}

/// A scan of namespace 0's forward lists from `from`, at `version`, at most `limit`.
fn forward_scan(from: Vec<u8>, version: u64, limit: usize) -> TestResult<KernelScan> {
    let limit = NonZeroUsize::new(limit).context("a limit")?;
    Ok(KernelScan::new(Keyspace::Forward, NS, from, version, limit))
}

fn forward_key(store: &TestQuads, subject: &str, predicate: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        namespace: NS,
        graph: Graph::Default,
        predicate: id(store, NS, &ex(predicate))?,
        subject: id(store, NS, &ex(subject))?,
    }
    .to_bytes())
}

/// The kernel's rows of `scan` and the key it resumes at.
fn scanned(store: &TestQuads, scan: &KernelScan) -> TestResult<(Vec<KernelRow>, Option<Vec<u8>>)> {
    let mut rows = vec![KernelRow::new(b"before".to_vec(), Vec::new())];
    let resume = store.scan(scan, &mut rows)?;
    let first = rows.remove(0);
    assert_eq!(first.key(), b"before", "rows are appended");
    Ok((rows, resume))
}

#[test]
fn test_the_kernel_reads_a_namespace_s_lists_in_key_order() -> TestResult {
    let store = quads()?;
    let at = write_at(
        &store,
        NS,
        &[
            quad("a", "knows", ex("b")),
            quad("a", "name", Literal::new_simple_literal("x")),
            quad("c", "knows", ex("d")),
        ],
    )?;
    let _other: u64 = write_at(&store, NS + 1, &[quad("e", "knows", ex("f"))])?;
    let mut expected = vec![
        (forward_key(&store, "a", "knows")?, Term::from(ex("b"))),
        (
            forward_key(&store, "a", "name")?,
            Literal::new_simple_literal("x").into(),
        ),
        (forward_key(&store, "c", "knows")?, Term::from(ex("d"))),
    ];
    expected.sort_by(|x, y| x.0.cmp(&y.0));
    let (rows, resume) = scanned(&store, &forward_scan(Vec::new(), u64::MAX, 10)?)?;
    assert_eq!(resume, None, "the namespace's keyspace is exhausted");
    let read: Vec<(Vec<u8>, Term)> = rows
        .iter()
        .map(|row| {
            let (object, _) = row.objects().first().context("one object")?;
            Ok((row.key().to_vec(), store.term_of(NS, object)?))
        })
        .collect::<TestResult<_>>()?;
    assert_eq!(read, expected);
    // Before the write the lists hold nothing, so no row.
    let (none, unread) = scanned(&store, &forward_scan(Vec::new(), at - 1, 10)?)?;
    assert!(none.is_empty());
    assert_eq!(unread, None);
    Ok(())
}

#[test]
fn test_a_scan_with_limit_one_resumes_where_it_stopped() -> TestResult {
    let store = quads()?;
    let _at: u64 = write_at(
        &store,
        NS,
        &[quad("a", "knows", ex("b")), quad("c", "knows", ex("d"))],
    )?;
    let mut keys = [
        forward_key(&store, "a", "knows")?,
        forward_key(&store, "c", "knows")?,
    ];
    keys.sort();
    let (first, resume) = scanned(&store, &forward_scan(Vec::new(), u64::MAX, 1)?)?;
    assert_eq!(
        first.iter().map(|r| r.key().to_vec()).collect::<Vec<_>>(),
        vec![keys[0].clone()]
    );
    let resume = resume.context("one list is left")?;
    let (second, end) = scanned(&store, &forward_scan(resume, u64::MAX, 1)?)?;
    assert_eq!(
        second.iter().map(|r| r.key().to_vec()).collect::<Vec<_>>(),
        vec![keys[1].clone()]
    );
    assert_eq!(end, None, "nothing is left after the last list");
    // A limit the range fills exactly leaves nothing to resume.
    let (both, none) = scanned(&store, &forward_scan(Vec::new(), u64::MAX, 2)?)?;
    assert_eq!(both.len(), 2);
    assert_eq!(none, None);
    Ok(())
}

#[test]
fn test_a_scan_is_bounded_by_its_end_and_by_the_namespace() -> TestResult {
    let store = quads()?;
    let _at: u64 = write_at(
        &store,
        NS,
        &[quad("a", "knows", ex("b")), quad("c", "knows", ex("d"))],
    )?;
    let _other: u64 = write_at(&store, NS + 1, &[quad("e", "knows", ex("f"))])?;
    let mut keys = [
        forward_key(&store, "a", "knows")?,
        forward_key(&store, "c", "knows")?,
    ];
    keys.sort();
    let (before_second, resume) = scanned(
        &store,
        &forward_scan(Vec::new(), u64::MAX, 10)?.to(keys[1].clone()),
    )?;
    assert_eq!(before_second.len(), 1, "the end is exclusive");
    assert_eq!(resume, None);
    // From the next namespace's first key on: past this namespace's end.
    let past = key_prefix(Keyspace::Forward, false, NS + 1, None, None);
    let (none, unread) = scanned(&store, &forward_scan(past, u64::MAX, 10)?)?;
    assert!(none.is_empty());
    assert_eq!(unread, None);
    Ok(())
}

#[test]
fn test_a_list_holding_nothing_at_the_version_gives_no_row() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, NS, &[quad("a", "knows", ex("b"))])?;
    let read_ts = store.lists().applied_ts();
    let mut write = QuadWrite::new(&store, NS, store.lists().request(read_ts));
    let (a, knows, b) = (
        id(&store, NS, &ex("a"))?,
        id(&store, NS, &ex("knows"))?,
        object(&store, "b")?,
    );
    let _deleted = write
        .delete(Graph::Default, a, knows, &b, &kind(&Term::from(ex("b"))))?
        .context("held")?;
    let deleted_at = commit(&store, vec![write])?
        .first()
        .copied()
        .context("commits")?;
    let (held, _) = scanned(&store, &forward_scan(Vec::new(), at, 10)?)?;
    assert_eq!(held.len(), 1);
    let (empty, resume) = scanned(&store, &forward_scan(Vec::new(), deleted_at, 10)?)?;
    assert!(empty.is_empty());
    assert_eq!(resume, None);
    Ok(())
}

#[test]
fn test_a_keyspace_holding_no_lists_is_refused() -> TestResult {
    let store = quads()?;
    for keyspace in [
        Keyspace::Marks,
        Keyspace::Referrers,
        Keyspace::Statements,
        Keyspace::Dictionary,
    ] {
        let scan = KernelScan::new(keyspace, NS, Vec::new(), u64::MAX, NonZeroUsize::MIN);
        let error = store
            .scan(&scan, &mut Vec::new())
            .err()
            .context("refused")?;
        let _corruption: (Vec<u8>, String) = corruption(error)?;
    }
    Ok(())
}
