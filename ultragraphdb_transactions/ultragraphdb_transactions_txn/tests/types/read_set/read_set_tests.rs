/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_mapping::{Cardinality, Change, Declaration, PredicateDeclaration};
use ultragraphdb_transactions_txn::{ReadSet, TxnError, TxnErrorEnum};

use crate::utils::fixture::{NS, Rng, applier, changeset, commit, ex, insert, insert_literal};

/// The catalog range of namespace `ns`, from the key layout: the catalog keyspace byte `0x01` and the namespace
/// big-endian, to the same prefix of the next namespace.
fn catalog_of(ns: u64) -> (Vec<u8>, Vec<u8>) {
    let mut start = vec![0x01];
    start.extend_from_slice(&ns.to_be_bytes());
    let mut end = vec![0x01];
    end.extend_from_slice(&(ns + 1).to_be_bytes());
    (start, end)
}

/// The bytes a fresh read set of a namespace accounts for: the catalog range's two 9-byte keys and 8 bytes for each.
const CATALOG_BYTES: usize = 9 + 9 + 16;

fn lists(r: &ReadSet) -> Vec<Vec<u8>> {
    r.lists().map(<[u8]>::to_vec).collect()
}

fn statements(r: &ReadSet) -> Vec<(Vec<u8>, Vec<u8>)> {
    r.statements()
        .map(|(k, o)| (k.to_vec(), o.to_vec()))
        .collect()
}

fn ranges(r: &ReadSet) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    r.ranges()
        .map(|(s, e)| (s.to_vec(), e.map(<[u8]>::to_vec)))
        .collect()
}

fn absent(r: &ReadSet) -> Vec<Vec<u8>> {
    r.absent().map(<[u8]>::to_vec).collect()
}

#[test]
fn test_a_reset_read_set_holds_its_namespaces_catalog_range_alone() {
    let mut r = ReadSet::new();
    r.reset(3, 40, 1_000);
    let (start, end) = catalog_of(3);
    assert_eq!(r.catalog(), (start.as_slice(), Some(end.as_slice())));
    assert_eq!((r.namespace(), r.read_version(), r.limit()), (3, 40, 1_000));
    assert_eq!(r.bytes(), CATALOG_BYTES);
    assert!(lists(&r).is_empty() && statements(&r).is_empty());
    assert!(ranges(&r).is_empty() && absent(&r).is_empty());
}

#[test]
fn test_the_last_namespaces_catalog_range_ends_at_the_next_keyspace() {
    // 0x01 then eight 0xff bytes: the next key after the prefix carries into the keyspace byte, 0x02.
    let mut r = ReadSet::new();
    r.reset(u64::MAX, 1, 1_000);
    let mut start = vec![0x01];
    start.extend_from_slice(&[0xff; 8]);
    assert_eq!(r.catalog(), (start.as_slice(), Some(&[0x02][..])));
}

#[test]
fn test_a_new_read_set_is_bounded_by_the_default_and_holds_namespace_zeros_catalog() {
    let r = ReadSet::new();
    let (start, end) = catalog_of(0);
    assert_eq!(r.catalog(), (start.as_slice(), Some(end.as_slice())));
    assert_eq!(r.limit(), 1_048_576);
    assert_eq!(r.bytes(), CATALOG_BYTES);
}

#[test]
fn test_each_push_is_accounted_as_its_bytes_and_8_per_key() {
    let mut r = ReadSet::new();
    r.reset(NS, 1, 10_000);
    r.push_list(b"list").unwrap();
    assert_eq!(r.bytes(), CATALOG_BYTES + 4 + 8);
    r.push_statement(b"key", b"object").unwrap();
    assert_eq!(r.bytes(), CATALOG_BYTES + 12 + 3 + 6 + 16);
    r.push_range(b"from", Some(b"to")).unwrap();
    assert_eq!(r.bytes(), CATALOG_BYTES + 37 + 4 + 2 + 16);
    r.push_range(b"open", None).unwrap();
    assert_eq!(r.bytes(), CATALOG_BYTES + 59 + 4 + 16);
    r.push_absent(b"gone").unwrap();
    assert_eq!(r.bytes(), CATALOG_BYTES + 79 + 4 + 8);
}

#[test]
fn test_a_push_at_the_limit_is_taken_and_one_past_it_is_refused_adding_nothing() {
    // CATALOG_BYTES + 5 + 8: a five-byte key fills the limit exactly.
    let limit = CATALOG_BYTES + 13;
    let mut r = ReadSet::new();
    r.reset(NS, 1, limit);
    r.push_list(b"12345").unwrap();
    assert_eq!(r.bytes(), limit);
    let refusals = [
        r.push_list(b"").unwrap_err(),
        r.push_statement(b"", b"").unwrap_err(),
        r.push_range(b"", None).unwrap_err(),
        r.push_absent(b"x").unwrap_err(),
    ];
    assert_eq!(
        refusals,
        [
            TxnError::ReadSetTooLarge(limit + 8, limit),
            TxnError::ReadSetTooLarge(limit + 16, limit),
            TxnError::ReadSetTooLarge(limit + 16, limit),
            TxnError::ReadSetTooLarge(limit + 9, limit),
        ]
    );
    assert_eq!(r.bytes(), limit);
    assert_eq!(lists(&r), vec![b"12345".to_vec()]);
    assert!(statements(&r).is_empty() && ranges(&r).is_empty() && absent(&r).is_empty());
}

#[test]
fn test_one_byte_under_the_limit_refuses_a_key_one_byte_too_long() {
    let limit = CATALOG_BYTES + 12;
    let mut r = ReadSet::new();
    r.reset(NS, 1, limit);
    assert!(matches!(
        r.push_list(b"12345").unwrap_err().0,
        TxnErrorEnum::ReadSetTooLarge { size, limit: l } if size == limit + 1 && l == limit
    ));
    r.push_list(b"1234").unwrap();
    assert_eq!(r.bytes(), limit);
}

#[test]
fn test_sealing_sorts_and_drops_duplicates() {
    let mut r = ReadSet::new();
    r.reset(NS, 1, 10_000);
    for key in [&b"b"[..], b"a", b"b", b"", b"ab"] {
        r.push_list(key).unwrap();
        r.push_absent(key).unwrap();
    }
    r.push_statement(b"k", b"2").unwrap();
    r.push_statement(b"k", b"1").unwrap();
    r.push_statement(b"j", b"9").unwrap();
    r.push_statement(b"k", b"1").unwrap();
    r.push_range(b"m", None).unwrap();
    r.push_range(b"m", Some(b"z")).unwrap();
    r.push_range(b"c", Some(b"d")).unwrap();
    r.push_range(b"m", None).unwrap();
    r.push_range(b"m", Some(b"n")).unwrap();
    assert!(!r.is_sealed());
    r.seal();
    assert!(r.is_sealed());
    let sorted = vec![b"".to_vec(), b"a".to_vec(), b"ab".to_vec(), b"b".to_vec()];
    assert_eq!(lists(&r), sorted);
    assert_eq!(absent(&r), sorted);
    assert_eq!(
        statements(&r),
        vec![
            (b"j".to_vec(), b"9".to_vec()),
            (b"k".to_vec(), b"1".to_vec()),
            (b"k".to_vec(), b"2".to_vec())
        ]
    );
    assert_eq!(
        ranges(&r),
        vec![
            (b"c".to_vec(), Some(b"d".to_vec())),
            (b"m".to_vec(), Some(b"n".to_vec())),
            (b"m".to_vec(), Some(b"z".to_vec())),
            (b"m".to_vec(), None)
        ]
    );
    // The catalog range stays, and accounting counts what was pushed.
    let (start, end) = catalog_of(NS);
    assert_eq!(r.catalog(), (start.as_slice(), Some(end.as_slice())));
}

#[test]
fn test_before_sealing_the_tables_keep_push_order() {
    let mut r = ReadSet::new();
    r.reset(NS, 1, 10_000);
    r.push_list(b"b").unwrap();
    r.push_list(b"a").unwrap();
    r.push_list(b"b").unwrap();
    assert_eq!(lists(&r), vec![b"b".to_vec(), b"a".to_vec(), b"b".to_vec()]);
}

#[test]
fn test_a_push_after_sealing_unseals() {
    let mut r = ReadSet::new();
    r.reset(NS, 1, 10_000);
    r.push_list(b"b").unwrap();
    r.seal();
    r.push_list(b"a").unwrap();
    assert!(!r.is_sealed());
    r.seal();
    assert_eq!(lists(&r), vec![b"a".to_vec(), b"b".to_vec()]);
}

#[test]
fn test_a_reset_clears_everything_and_unseals() {
    let mut r = ReadSet::new();
    r.reset(NS, 1, 10_000);
    r.push_list(b"a").unwrap();
    r.push_statement(b"k", b"o").unwrap();
    r.push_range(b"s", None).unwrap();
    r.push_absent(b"d").unwrap();
    r.seal();
    r.reset(4, 9, 500);
    assert!(!r.is_sealed());
    assert_eq!(r.bytes(), CATALOG_BYTES);
    assert!(lists(&r).is_empty() && statements(&r).is_empty());
    assert!(ranges(&r).is_empty() && absent(&r).is_empty());
    let (start, end) = catalog_of(4);
    assert_eq!(r.catalog(), (start.as_slice(), Some(end.as_slice())));
}

/// A range's end in the model: a bounded end sorts before the open one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum End {
    Bounded(Vec<u8>),
    Open,
}

/// The read set's reference model: sets, and the accounting written out from the documented rule.
#[derive(Debug, Default)]
struct Model {
    lists: BTreeSet<Vec<u8>>,
    statements: BTreeSet<(Vec<u8>, Vec<u8>)>,
    ranges: BTreeSet<(Vec<u8>, End)>,
    absent: BTreeSet<Vec<u8>>,
    bytes: usize,
}

#[test]
fn test_the_sealed_read_set_agrees_with_a_set_model_over_random_pushes() {
    let mut r = ReadSet::with_capacity(64, 4);
    for seed in 0..200u64 {
        let mut rng = Rng::new(seed);
        // Small limits so refusals happen, large ones so they do not.
        let limit = CATALOG_BYTES + rng.below(400) as usize;
        let ns = rng.below(4);
        r.reset(ns, seed, limit);
        let mut model = Model {
            bytes: CATALOG_BYTES,
            ..Model::default()
        };
        for _ in 0..rng.below(60) {
            let kind = rng.below(5);
            let (a, b) = (rng.key(6), rng.key(6));
            let cost = match kind {
                0 | 3 => a.len() + 8,
                1 => a.len() + b.len() + 16,
                2 => a.len() + b.len() + 16,
                _ => a.len() + 16,
            };
            let result = match kind {
                0 => r.push_list(&a),
                1 => r.push_statement(&a, &b),
                2 => r.push_range(&a, Some(&b)),
                3 => r.push_absent(&a),
                _ => r.push_range(&a, None),
            };
            if model.bytes + cost > limit {
                assert_eq!(
                    result,
                    Err(TxnError::ReadSetTooLarge(model.bytes + cost, limit))
                );
            } else {
                assert_eq!(result, Ok(()), "seed {seed}");
                model.bytes += cost;
                match kind {
                    0 => drop(model.lists.insert(a)),
                    1 => drop(model.statements.insert((a, b))),
                    2 => drop(model.ranges.insert((a, End::Bounded(b)))),
                    3 => drop(model.absent.insert(a)),
                    _ => drop(model.ranges.insert((a, End::Open))),
                }
            }
            assert_eq!(r.bytes(), model.bytes, "seed {seed}");
        }
        r.seal();
        assert_eq!(
            lists(&r),
            model.lists.iter().cloned().collect::<Vec<_>>(),
            "seed {seed}"
        );
        assert_eq!(
            statements(&r),
            model.statements.iter().cloned().collect::<Vec<_>>()
        );
        assert_eq!(absent(&r), model.absent.iter().cloned().collect::<Vec<_>>());
        let expected: Vec<_> = model
            .ranges
            .iter()
            .map(|(s, e)| {
                (
                    s.clone(),
                    match e {
                        End::Bounded(e) => Some(e.clone()),
                        End::Open => None,
                    },
                )
            })
            .collect();
        assert_eq!(ranges(&r), expected, "seed {seed}");
        let (start, end) = catalog_of(ns);
        assert_eq!(r.catalog(), (start.as_slice(), Some(end.as_slice())));
    }
}

#[test]
fn test_a_reused_read_set_keeps_its_capacity_and_grows_no_more_for_what_fits() {
    let mut r = ReadSet::new();
    let fill = |r: &mut ReadSet| {
        r.reset(NS, 1, 1 << 20);
        for i in 0u32..100 {
            r.push_list(&i.to_be_bytes()).unwrap();
            r.push_statement(&i.to_be_bytes(), b"object").unwrap();
            r.push_range(&i.to_be_bytes(), None).unwrap();
            r.push_absent(&i.to_le_bytes()).unwrap();
        }
        r.seal();
    };
    fill(&mut r);
    let grown = r.capacity();
    assert!(
        grown.0 >= 18 + 100 * (4 + 4 + 6 + 4 + 4) && grown.1 >= 400,
        "{grown:?}"
    );
    for _ in 0..3 {
        r.reset(NS, 2, 1 << 20);
        assert_eq!(r.capacity(), grown, "a reset keeps the buffers");
        fill(&mut r);
        assert_eq!(r.capacity(), grown, "what fits takes no more room");
    }
}

#[test]
fn test_with_capacity_reserves_its_buffers() {
    let r = ReadSet::with_capacity(4_096, 32);
    let (bytes, keys) = r.capacity();
    assert!(bytes >= 4_096, "{bytes}");
    assert!(keys >= 4 * 32, "{keys}");
    assert_eq!(r.bytes(), CATALOG_BYTES);
}

#[test]
fn test_gathering_an_executions_reads_records_each_of_them() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "z")]);
    let executed = applier
        .execute(&changeset(vec![
            insert("a", "knows", "b"),
            insert("c", "likes", "d"),
        ]))
        .expect("executed");
    let reads = executed.reads();
    assert!(!reads.statements().is_empty() && !reads.absent().is_empty());
    let mut r = ReadSet::new();
    r.reset(NS, executed.read_ts(), 1 << 20);
    r.gather(reads).unwrap();
    r.seal();
    assert_eq!(lists(&r), reads.lists().to_vec());
    assert_eq!(statements(&r), reads.statements().to_vec());
    assert_eq!(ranges(&r), reads.ranges().to_vec());
    assert_eq!(absent(&r), reads.absent().to_vec());
    let (start, end) = reads.catalog();
    assert_eq!(r.catalog(), (start.as_slice(), end.as_deref()));
}

#[test]
fn test_gathering_a_declarations_scan_records_its_ranges() {
    // Narrowing a predicate that holds data scans its lists: the execution's reads hold ranges, and so must the set.
    let applier = applier();
    commit(&applier, vec![insert_literal("a", "nick", "x")]);
    let single = PredicateDeclaration::new(ex("nick")).with_cardinality(Cardinality::Single);
    let executed = applier
        .execute(&changeset(vec![Change::Declare(Declaration::Predicate(
            single,
        ))]))
        .expect("executed");
    let reads = executed.reads();
    assert!(
        !reads.ranges().is_empty(),
        "the declaration scans its predicate's data"
    );
    let mut r = ReadSet::new();
    r.reset(NS, executed.read_ts(), 1 << 20);
    r.gather(reads).unwrap();
    r.seal();
    let gathered: BTreeSet<_> = ranges(&r).into_iter().collect();
    let expected: BTreeSet<_> = reads.ranges().iter().cloned().collect();
    assert_eq!(gathered, expected);
}

#[test]
fn test_a_limit_past_u32_is_capped_where_spans_reach() {
    let mut r = ReadSet::new();
    r.reset(NS, 1, usize::MAX);
    assert_eq!(r.limit(), u32::MAX as usize);
}

#[test]
fn test_a_gather_past_the_limit_leaves_the_read_set_as_it_was() {
    let applier = applier();
    let executed = applier
        .execute(&changeset(vec![insert("a", "knows", "b")]))
        .expect("executed");
    let mut r = ReadSet::new();
    r.reset(NS, 1, CATALOG_BYTES + 20);
    r.push_list(b"before").unwrap();
    let error = r.gather(executed.reads()).unwrap_err();
    assert!(
        matches!(error.0, TxnErrorEnum::ReadSetTooLarge { limit, .. } if limit == CATALOG_BYTES + 20)
    );
    assert_eq!(r.bytes(), CATALOG_BYTES + 14);
    assert_eq!(lists(&r), vec![b"before".to_vec()]);
    assert!(statements(&r).is_empty() && absent(&r).is_empty());
}
