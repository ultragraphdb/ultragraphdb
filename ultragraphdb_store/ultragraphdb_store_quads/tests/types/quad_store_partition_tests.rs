/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The reads a partition answers beside its quad reads, each at a version: referrer sets and marks from values written
//! by hand, its own reverse entries behind the object's predicate list, its predicate lists, reverse counts, and the
//! read kernel over a keyspace's range.

use std::collections::BTreeMap;
use std::num::{NonZeroU64, NonZeroUsize};
use std::ops::ControlFlow;

use oxrdf::{Literal, Term};
use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_posting::{DEL, PostingIdentity};
use ultragraphdb_store_quads::{
    KernelRow, KernelScan, PageEnd, QuadErrorEnum, QuadWrite, ReferrerMember, StatementRecord,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::utils::helpers::{TestQuads, commit, ex, id, kind, quad, quads, write, write_at};

const TENANT: u64 = 0;

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
        .map(|n| id(store, TENANT, &ex(n)))
        .collect::<TestResult<Vec<_>>>()?;
    ids.sort_by_key(|i| i.to_u128());
    Ok(ids)
}

fn object(store: &TestQuads, name: &str) -> TestResult<EncodedTerm> {
    Ok(EncodedTerm::id(id(store, TENANT, &ex(name))?))
}

#[test]
fn test_referrers_read_the_set_written_at_or_below_the_version() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, TENANT, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, TENANT, &ex("o"))?;
    let key = QuadKey::Referrers {
        tenant: TENANT,
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
        store.referrers(TENANT, o, at + 99)?.members().is_empty(),
        "none yet"
    );
    assert_eq!(
        store.referrers(TENANT, o, at + 100)?.members(),
        &[member(7, 12, 4)?]
    );
    assert_eq!(
        store.referrers(TENANT, o, u64::MAX)?.members(),
        &[member(3, 13, 1)?, member(7, 12, 4)?]
    );
    // Another tenant's set of the same ID is another key.
    assert!(
        store
            .referrers(TENANT + 1, o, u64::MAX)?
            .members()
            .is_empty()
    );
    Ok(())
}

#[test]
fn test_a_referrer_set_value_that_does_not_decode_is_corruption_naming_its_key() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, TENANT, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, TENANT, &ex("o"))?;
    let key = QuadKey::Referrers {
        tenant: TENANT,
        entity: o,
    }
    .to_bytes();
    put(&store, key.clone(), at + 1, vec![9])?;
    let error = store
        .referrers(TENANT, o, u64::MAX)
        .err()
        .context("refused")?;
    assert_eq!(corruption(error)?.0, key);
    Ok(())
}

#[test]
fn test_a_mark_reads_its_generation_at_or_below_the_version() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, TENANT, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, TENANT, &ex("o"))?;
    let (seven, nine) = (BucketGroup::new(7)?, BucketGroup::new(9)?);
    let key = QuadKey::Mark {
        tenant: TENANT,
        entity: o,
        group: 7,
    }
    .to_bytes();
    put(&store, key.clone(), at + 10, vec![0, 0, 0, 0, 0, 0, 0, 5])?;
    put(&store, key, at + 20, vec![0, 0, 0, 0, 0, 0, 1, 0])?;
    assert_eq!(store.mark(TENANT, o, seven, at + 9)?, None, "absent before");
    assert_eq!(store.mark(TENANT, o, seven, at + 10)?, NonZeroU64::new(5));
    assert_eq!(store.mark(TENANT, o, seven, at + 20)?, NonZeroU64::new(256));
    assert_eq!(
        store.mark(TENANT, o, nine, u64::MAX)?,
        None,
        "another group's mark"
    );
    Ok(())
}

#[test]
fn test_a_mark_that_is_no_generation_of_at_least_one_is_corruption() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, TENANT, &[quad("a", "knows", ex("o"))])?;
    let o = id(&store, TENANT, &ex("o"))?;
    let group = BucketGroup::new(BucketGroup::COUNT - 1)?;
    let key = QuadKey::Mark {
        tenant: TENANT,
        entity: o,
        group: BucketGroup::COUNT - 1,
    }
    .to_bytes();
    put(&store, key.clone(), at + 1, vec![0; 8])?;
    let zero = store
        .mark(TENANT, o, group, at + 1)
        .err()
        .context("a zero")?;
    assert_eq!(corruption(zero)?.0, key);
    put(&store, key.clone(), at + 2, vec![0, 0, 0, 0, 0, 0, 1])?;
    let short = store
        .mark(TENANT, o, group, at + 2)
        .err()
        .context("7 bytes")?;
    assert_eq!(corruption(short)?.0, key);
    Ok(())
}

#[test]
fn test_subjects_and_counts_are_the_partition_s_own_holders_of_the_object() -> TestResult {
    let store = quads()?;
    let first = write_at(&store, TENANT, &[quad("a", "knows", ex("b"))])?;
    let second = write_at(
        &store,
        TENANT,
        &[quad("c", "knows", ex("b")), quad("c", "likes", ex("b"))],
    )?;
    let knows = id(&store, TENANT, &ex("knows"))?;
    let b = object(&store, "b")?;
    assert_eq!(
        store.subjects(TENANT, Graph::Default, knows, &b, first)?,
        ids(&store, &["a"])?
    );
    assert_eq!(
        store.subjects(TENANT, Graph::Default, knows, &b, second)?,
        ids(&store, &["a", "c"])?
    );
    assert_eq!(
        store.reverse_count(TENANT, Graph::Default, knows, &b, first)?,
        1
    );
    assert_eq!(
        store.reverse_count(TENANT, Graph::Default, knows, &b, second)?,
        2
    );
    // An object no subject points at with the predicate.
    let a = object(&store, "a")?;
    assert!(
        store
            .subjects(TENANT, Graph::Default, knows, &a, second)?
            .is_empty()
    );
    assert_eq!(
        store.reverse_count(TENANT, Graph::Default, knows, &a, second)?,
        0
    );
    // A literal object, which has its reverse entries too: its encoding as the forward list holds it.
    let at = write_at(
        &store,
        TENANT,
        &[quad("d", "name", Literal::new_simple_literal("x"))],
    )?;
    let name = id(&store, TENANT, &ex("name"))?;
    let (x, _) = store
        .objects(&forward_key(&store, "d", "name")?, at)?
        .into_iter()
        .next()
        .context("d holds x")?;
    assert_eq!(
        store.subjects(TENANT, Graph::Default, name, &x, at)?,
        ids(&store, &["d"])?
    );
    Ok(())
}

#[test]
fn test_a_reverse_list_the_object_s_predicate_list_does_not_name_is_not_read() -> TestResult {
    let store = quads()?;
    let _at: u64 = write_at(&store, TENANT, &[quad("a", "knows", ex("b"))])?;
    let knows = id(&store, TENANT, &ex("knows"))?;
    let b = object(&store, "b")?;
    // The object-predicates list loses `knows` while the reverse list keeps `a`: only the predicate list is consulted.
    let read_ts = store.lists().applied_ts();
    let mut write = QuadWrite::new(&store, TENANT, store.lists().request(read_ts));
    let op = QuadKey::ObjectPredicates {
        tenant: TENANT,
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
        tenant: TENANT,
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
            .object_predicates(TENANT, Graph::Default, &b, at)?
            .is_empty()
    );
    assert!(
        store
            .subjects(TENANT, Graph::Default, knows, &b, at)?
            .is_empty()
    );
    assert_eq!(
        store.reverse_count(TENANT, Graph::Default, knows, &b, at)?,
        0
    );
    Ok(())
}

#[test]
fn test_predicate_lists_name_a_node_s_predicates_at_the_version() -> TestResult {
    let store = quads()?;
    let first = write_at(&store, TENANT, &[quad("a", "knows", ex("b"))])?;
    let second = write_at(
        &store,
        TENANT,
        &[
            quad("a", "name", Literal::new_simple_literal("x")),
            quad("c", "likes", ex("b")),
        ],
    )?;
    let a = id(&store, TENANT, &ex("a"))?;
    let b = object(&store, "b")?;
    assert_eq!(
        store.subject_predicates(TENANT, Graph::Default, a, first)?,
        ids(&store, &["knows"])?
    );
    assert_eq!(
        store.subject_predicates(TENANT, Graph::Default, a, second)?,
        ids(&store, &["knows", "name"])?
    );
    assert_eq!(
        store.object_predicates(TENANT, Graph::Default, &b, first)?,
        ids(&store, &["knows"])?
    );
    assert_eq!(
        store.object_predicates(TENANT, Graph::Default, &b, second)?,
        ids(&store, &["knows", "likes"])?
    );
    // A node without lists, and a version before any.
    let c = id(&store, TENANT, &ex("c"))?;
    assert!(
        store
            .subject_predicates(TENANT, Graph::Default, c, first)?
            .is_empty()
    );
    assert!(
        store
            .object_predicates(TENANT, Graph::Default, &EncodedTerm::id(a), second)?
            .is_empty()
    );
    Ok(())
}

/// A scan of tenant 0's forward lists from `from`, at `version`, at most `limit`.
fn forward_scan(from: Vec<u8>, version: u64, limit: usize) -> TestResult<KernelScan> {
    let limit = NonZeroUsize::new(limit).context("a limit")?;
    Ok(KernelScan::new(
        Keyspace::Forward,
        TENANT,
        from,
        version,
        limit,
    ))
}

fn forward_key(store: &TestQuads, subject: &str, predicate: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        tenant: TENANT,
        graph: Graph::Default,
        predicate: id(store, TENANT, &ex(predicate))?,
        subject: id(store, TENANT, &ex(subject))?,
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
fn test_the_kernel_reads_a_tenant_s_lists_in_key_order() -> TestResult {
    let store = quads()?;
    let at = write_at(
        &store,
        TENANT,
        &[
            quad("a", "knows", ex("b")),
            quad("a", "name", Literal::new_simple_literal("x")),
            quad("c", "knows", ex("d")),
        ],
    )?;
    let _other: u64 = write_at(&store, TENANT + 1, &[quad("e", "knows", ex("f"))])?;
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
    assert_eq!(resume, None, "the tenant's keyspace is exhausted");
    let read: Vec<(Vec<u8>, Term)> = rows
        .iter()
        .map(|row| {
            let (object, _) = row.objects().first().context("one object")?;
            Ok((row.key().to_vec(), store.term_of(TENANT, object)?))
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
        TENANT,
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
fn test_a_scan_is_bounded_by_its_end_and_by_the_tenant() -> TestResult {
    let store = quads()?;
    let _at: u64 = write_at(
        &store,
        TENANT,
        &[quad("a", "knows", ex("b")), quad("c", "knows", ex("d"))],
    )?;
    let _other: u64 = write_at(&store, TENANT + 1, &[quad("e", "knows", ex("f"))])?;
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
    // From the next tenant's first key on: past this tenant's end.
    let past = key_prefix(Keyspace::Forward, false, TENANT + 1, None, None);
    let (none, unread) = scanned(&store, &forward_scan(past, u64::MAX, 10)?)?;
    assert!(none.is_empty());
    assert_eq!(unread, None);
    Ok(())
}

#[test]
fn test_a_list_holding_nothing_at_the_version_gives_no_row() -> TestResult {
    let store = quads()?;
    let at = write_at(&store, TENANT, &[quad("a", "knows", ex("b"))])?;
    let read_ts = store.lists().applied_ts();
    let mut write = QuadWrite::new(&store, TENANT, store.lists().request(read_ts));
    let (a, knows, b) = (
        id(&store, TENANT, &ex("a"))?,
        id(&store, TENANT, &ex("knows"))?,
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
        let scan = KernelScan::new(keyspace, TENANT, Vec::new(), u64::MAX, NonZeroUsize::MIN);
        let error = store
            .scan(&scan, &mut Vec::new())
            .err()
            .context("refused")?;
        let _corruption: (Vec<u8>, String) = corruption(error)?;
    }
    Ok(())
}

/// The referrer set `value` of `entity`, written under its key at `version`.
fn referrers(
    store: &TestQuads,
    entity: SystemId,
    version: u64,
    value: &[u8],
) -> TestResult<Vec<u8>> {
    let key = QuadKey::Referrers {
        tenant: TENANT,
        entity,
    }
    .to_bytes();
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(key.clone(), version, 0, value.to_vec()));
    store.store().apply(batch, PersistMode::Sync)?;
    Ok(key)
}

/// A key and its stored value.
type Stored = (Vec<u8>, Vec<u8>);

/// Three referrer keys in key order, and the version the last was written at.
fn referrer_store() -> TestResult<(TestQuads, Vec<Stored>, u64)> {
    let store = quads()?;
    let at = write_at(
        &store,
        TENANT,
        &[
            quad("a", "knows", ex("x")),
            quad("b", "knows", ex("y")),
            quad("c", "knows", ex("z")),
        ],
    )?;
    // Any bytes: the page gives what is stored, and a reader decodes it.
    let version = at + 100;
    let mut held: Vec<Stored> = ["x", "y", "z"]
        .iter()
        .map(|name| {
            let value = name.as_bytes().to_vec();
            Ok((
                referrers(&store, id(&store, TENANT, &ex(name))?, version, &value)?,
                value,
            ))
        })
        .collect::<TestResult<_>>()?;
    held.sort();
    Ok((store, held, version))
}

fn referrer_scan(from: Vec<u8>, version: u64, limit: usize) -> TestResult<KernelScan> {
    Ok(KernelScan::new(
        Keyspace::Referrers,
        TENANT,
        from,
        version,
        NonZeroUsize::new(limit).context("a limit")?,
    ))
}

#[test]
fn test_a_values_page_streams_each_key_s_stored_value_at_its_version() -> TestResult {
    let (store, held, version) = referrer_store()?;
    let mut read = Vec::new();
    // A limit above the keys held: the page ends by exhausting its range, and a page giving more stops.
    let page = store.values_page(
        &referrer_scan(Vec::new(), version, held.len() + 1)?,
        |k, v| {
            read.push((k.to_vec(), v.to_vec()));
            ControlFlow::Continue(())
        },
    )?;
    assert_eq!(read, held);
    assert_eq!((page.end(), page.rows()), (&PageEnd::Exhausted, held.len()));
    // Below the version they were written at, the sets are absent.
    let mut below = 0;
    let none = store.values_page(&referrer_scan(Vec::new(), version - 1, 8)?, |_, _| {
        below += 1;
        ControlFlow::Continue(())
    })?;
    assert_eq!(
        (none.end(), none.rows(), below),
        (&PageEnd::Exhausted, 0, 0)
    );
    Ok(())
}

#[test]
fn test_a_values_page_stops_at_its_limit_and_pauses_between_keys() -> TestResult {
    let (store, held, version) = referrer_store()?;
    let [(first, _), (second, _), _] = held.as_slice() else {
        return Err(TestError::new("three keys"));
    };
    let stopped = store.values_page(&referrer_scan(Vec::new(), version, 1)?, |_, _| {
        ControlFlow::Continue(())
    })?;
    assert_eq!(
        (stopped.end(), stopped.rows()),
        (
            &PageEnd::Stopped {
                last: first.clone()
            },
            1
        )
    );

    let scan = referrer_scan(Vec::new(), version, usize::MAX)?;
    let nothing = store.values_page(&scan, |_, _| ControlFlow::Break(()))?;
    assert_eq!(
        (nothing.end(), nothing.rows()),
        (
            &PageEnd::Paused {
                last: None,
                from: first.clone(),
                after: None,
            },
            0
        )
    );

    let mut taken = 0;
    let paused = store.values_page(&scan, |_, _| {
        if taken == 2 {
            return ControlFlow::Break(());
        }
        taken += 1;
        ControlFlow::Continue(())
    })?;
    let mut past = second.clone();
    past.push(0);
    assert_eq!(
        (paused.end(), paused.rows()),
        (
            &PageEnd::Paused {
                last: Some(second.clone()),
                from: past.clone(),
                after: None,
            },
            2
        )
    );
    // The next page from where it paused reads the rest.
    let mut rest = Vec::new();
    let end = store.values_page(&referrer_scan(past, version, held.len() + 1)?, |k, _| {
        rest.push(k.to_vec());
        ControlFlow::Continue(())
    })?;
    assert_eq!(end.end(), &PageEnd::Exhausted);
    assert_eq!(rest, vec![held.last().context("a last key")?.0.clone()]);
    Ok(())
}

/// A store's keys and stored values.
type Held = Vec<(Vec<u8>, Vec<u8>)>;

/// Three statement records, of sequences 1 to 3, written at the version returned: their keys and stored bytes, in key order.
fn statement_store() -> TestResult<(TestQuads, Held, u64)> {
    let store = quads()?;
    write(&store, TENANT, &[quad("a", "knows", ex("b"))])?;
    let (a, knows, b) = (
        id(&store, TENANT, &ex("a"))?,
        id(&store, TENANT, &ex("knows"))?,
        EncodedTerm::id(id(&store, TENANT, &ex("b"))?),
    );
    let mut w = QuadWrite::new(
        &store,
        TENANT,
        store.lists().request(store.lists().applied_ts()),
    );
    let mut held = Vec::new();
    for sequence in 1..=3 {
        let statement = SystemId::new(TypePrefix::STATEMENT, a.bucket(), Sequence::new(sequence)?);
        let record = StatementRecord::new(a, knows, b.clone(), Vec::new());
        w.put_statement(Graph::Default, statement, &record);
        let key = QuadKey::Statement {
            tenant: TENANT,
            graph: Graph::Default,
            statement,
        }
        .to_bytes();
        held.push((key, record.encode()));
    }
    let version = *commit(&store, vec![w])?.first().context("a commit")?;
    Ok((store, held, version))
}

/// The statements of `TENANT` at `version`, at a limit above any page these tests read: such a page ends by exhausting its range,
/// and a page giving more than the store holds stops.
fn statement_scan(version: u64) -> TestResult<KernelScan> {
    Ok(KernelScan::new(
        Keyspace::Statements,
        TENANT,
        Vec::new(),
        version,
        NonZeroUsize::new(8).context("a limit")?,
    ))
}

/// The statements are read as stored bytes at the version, none below it.
#[test]
fn test_a_values_page_reads_the_statements_stored_bytes_at_its_version() -> TestResult {
    let (store, held, version) = statement_store()?;
    let mut read = Vec::new();
    let page = store.values_page(&statement_scan(version)?, |k, v| {
        read.push((k.to_vec(), v.to_vec()));
        ControlFlow::Continue(())
    })?;
    assert_eq!(read, held);
    assert_eq!((page.end(), page.rows()), (&PageEnd::Exhausted, held.len()));
    let below = store.values_page(&statement_scan(version - 1)?, |_, _| ControlFlow::Break(()))?;
    assert_eq!(below.end(), &PageEnd::Exhausted);
    Ok(())
}

/// What a page is given over the store: a value replacing the stored one, a removal hiding it, one the store lacks in its key
/// order, and those outside the range ignored; its limit counts what it gives.
#[test]
fn test_a_values_page_over_its_own_values_reads_them_over_the_store() -> TestResult {
    let (store, held, version) = statement_store()?;
    let [(first, _), (second, stored), (third, _)] = held.as_slice() else {
        return Err(TestError::new("three keys"));
    };
    let mut between = first.clone();
    between.push(0);
    let mut own = BTreeMap::new();
    let _replaced = own.insert(first.clone(), Some(b"mine".to_vec()));
    let _added = own.insert(between.clone(), Some(b"new".to_vec()));
    let _removed = own.insert(third.clone(), None);
    let mut past = third.clone();
    past.push(0);
    let _outside = own.insert(past.clone(), Some(b"outside".to_vec()));
    let scan = statement_scan(version)?.to(past.clone());
    let mut read = Vec::new();
    let page = store.values_page_over(&scan, &own, |k, v| {
        read.push((k.to_vec(), v.to_vec()));
        ControlFlow::Continue(())
    })?;
    assert_eq!(
        read,
        vec![
            (first.clone(), b"mine".to_vec()),
            (between.clone(), b"new".to_vec()),
            (second.clone(), stored.clone()),
        ]
    );
    assert_eq!((page.end(), page.rows()), (&PageEnd::Exhausted, 3));
    let limited = KernelScan::new(
        Keyspace::Statements,
        TENANT,
        Vec::new(),
        version,
        NonZeroUsize::new(2).context("a limit")?,
    );
    let stopped = store.values_page_over(&limited, &own, |_, _| ControlFlow::Continue(()))?;
    assert_eq!(
        (stopped.end(), stopped.rows()),
        (&PageEnd::Stopped { last: between }, 2)
    );
    Ok(())
}

#[test]
fn test_a_page_refuses_the_statements() -> TestResult {
    let store = quads()?;
    let refused = store
        .page(&statement_scan(u64::MAX)?, |_, _, _| {
            ControlFlow::Continue(())
        })
        .err()
        .context("the statements hold no lists")?;
    assert!(format!("{refused:?}").contains("lists"), "{refused:?}");
    Ok(())
}

/// A Scan's `after` names an object by its encoding: an ID by its 128 bits, a literal by its fingerprint and, where the keyspace
/// compares values, its encoding; bytes that are not one whole encoding are refused.
#[test]
fn test_a_resume_at_an_object_takes_the_identity_of_its_posting() -> TestResult {
    let store = quads()?;
    let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(1)?, Sequence::new(3)?);
    let at = store.resume_at(Keyspace::Forward, EncodedTerm::id(id).as_bytes())?;
    assert_eq!(
        at,
        PostingIdentity::new(id.to_u128(), Vec::new()).included()
    );
    let literal = EncodedTerm::literal(Literal::new_simple_literal("v0").as_ref(), |_| {
        Err(TermError::malformed(Vec::new(), "no datatype"))
    })?
    .into_term();
    let fingerprint = u128::from(store.fingerprint()(literal.as_bytes()));
    assert_eq!(
        store.resume_at(Keyspace::Forward, literal.as_bytes())?,
        PostingIdentity::new(fingerprint, literal.as_bytes().to_vec()).included()
    );
    assert_eq!(
        store.resume_at(Keyspace::VectorIndex, literal.as_bytes())?,
        PostingIdentity::new(fingerprint, Vec::new()).included()
    );
    let whole = EncodedTerm::id(id);
    for malformed in [
        &[][..],
        &whole.as_bytes()[..3],
        &[whole.as_bytes(), &[0][..]].concat()[..],
    ] {
        assert!(
            store.resume_at(Keyspace::Forward, malformed).is_err(),
            "{malformed:?}"
        );
    }
    Ok(())
}

#[test]
fn test_a_values_page_reads_stored_values_alone_and_a_page_reads_lists_alone() -> TestResult {
    let store = quads()?;
    let lists = forward_scan(Vec::new(), u64::MAX, 1)?;
    let refused_lists = store
        .values_page(&lists, |_, _| ControlFlow::Continue(()))
        .err()
        .context("a list's keyspace")?;
    assert!(
        format!("{refused_lists:?}").contains("statements"),
        "{refused_lists:?}"
    );
    let refused = store
        .page(&referrer_scan(Vec::new(), u64::MAX, 1)?, |_, _, _| {
            ControlFlow::Continue(())
        })
        .err()
        .context("the referrers' keyspace")?;
    assert!(format!("{refused:?}").contains("lists"), "{refused:?}");
    Ok(())
}
