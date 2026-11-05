/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use ultragraphdb_tool_bdd::{Context, TestResult};

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_mapping::{Cardinality, Change, Declaration, PredicateDeclaration};
use ultragraphdb_transactions_txn::{ReadSet, TxnError, TxnErrorEnum};

use crate::utils::fixture::{Rng, TENANT, applier, changeset, ex, insert, insert_literal, store};

/// The bytes a fresh read set accounts for: nothing, since the catalog is read through its version.
const CATALOG_BYTES: usize = 0;

/// The system ID of type 1, sequence `sequence` and bucket 5: `(1 << 112) | (sequence << 48) | (5 << 24)`.
fn id(sequence: u64) -> TestResult<SystemId> {
    SystemId::from_u128((1 << 112) | (u128::from(sequence) << 48) | (5 << 24)).context("the ID")
}

/// The term key of `id(sequence)`, written from the layout: the tag 0xFE, then the ID's 16 big-endian bytes, type 1
/// (`00 01`), the sequence's 8 bytes, bucket 5 (`00 00 05`) and 3 zero bytes.
fn term_key(sequence: u64) -> Vec<u8> {
    let mut key = vec![0xFE, 0x00, 0x01];
    key.extend_from_slice(&sequence.to_be_bytes());
    key.extend_from_slice(&[0, 0, 5, 0, 0, 0]);
    key
}

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
fn test_a_reset_read_set_holds_nothing() -> TestResult {
    let mut r = ReadSet::new();
    r.push_list(b"before")?;
    r.reset(3, 40, 1_000);
    assert_eq!((r.tenant(), r.read_version(), r.limit()), (3, 40, 1_000));
    assert_eq!(r.bytes(), 0);
    assert!(lists(&r).is_empty() && statements(&r).is_empty());
    assert!(ranges(&r).is_empty() && absent(&r).is_empty());
    Ok(())
}

#[test]
fn test_a_new_read_set_is_bounded_by_the_default_and_empty() {
    let r = ReadSet::new();
    assert_eq!(r.limit(), 1_048_576);
    assert_eq!(r.bytes(), 0);
    assert_eq!((r.tenant(), r.read_version()), (0, 0));
}

#[test]
fn test_an_absent_term_is_recorded_by_its_term_key() -> TestResult {
    let mut r = ReadSet::new();
    r.push_absent(id(7)?)?;
    assert_eq!(absent(&r), vec![term_key(7)]);
    // 17 bytes of key and 8 of its span.
    assert_eq!(r.bytes(), 25);
    assert_eq!(ReadSet::term_key(id(7)?).to_vec(), term_key(7));
    Ok(())
}

#[test]
fn test_each_push_is_accounted_as_its_bytes_and_8_per_key() -> TestResult {
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, 10_000);
    r.push_list(b"list")?;
    assert_eq!(r.bytes(), CATALOG_BYTES + 4 + 8);
    r.push_statement(b"key", b"object")?;
    assert_eq!(r.bytes(), CATALOG_BYTES + 12 + 3 + 6 + 16);
    r.push_range(b"from", Some(b"to"))?;
    assert_eq!(r.bytes(), CATALOG_BYTES + 37 + 4 + 2 + 16);
    r.push_range(b"open", None)?;
    assert_eq!(r.bytes(), CATALOG_BYTES + 59 + 4 + 16);
    r.push_absent(id(1)?)?;
    assert_eq!(r.bytes(), CATALOG_BYTES + 79 + 17 + 8);
    Ok(())
}

#[test]
fn test_a_push_at_the_limit_is_taken_and_one_past_it_is_refused_adding_nothing() -> TestResult {
    // CATALOG_BYTES + 5 + 8: a five-byte key fills the limit exactly.
    let limit = CATALOG_BYTES + 13;
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, limit);
    r.push_list(b"12345")?;
    assert_eq!(r.bytes(), limit);
    let refusals = [
        r.push_list(b"").err().context("the call fails")?,
        r.push_statement(b"", b"").err().context("the call fails")?,
        r.push_range(b"", None).err().context("the call fails")?,
        r.push_absent(id(1)?).err().context("the call fails")?,
    ];
    assert_eq!(
        refusals,
        [
            TxnError::read_set_too_large(limit + 8, limit),
            TxnError::read_set_too_large(limit + 16, limit),
            TxnError::read_set_too_large(limit + 16, limit),
            TxnError::read_set_too_large(limit + 25, limit),
        ]
    );
    assert_eq!(r.bytes(), limit);
    assert_eq!(lists(&r), vec![b"12345".to_vec()]);
    assert!(statements(&r).is_empty() && ranges(&r).is_empty() && absent(&r).is_empty());
    Ok(())
}

#[test]
fn test_one_byte_under_the_limit_refuses_a_key_one_byte_too_long() -> TestResult {
    let limit = CATALOG_BYTES + 12;
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, limit);
    assert!(matches!(
        r.push_list(b"12345").err().context("the call fails")?.0,
        TxnErrorEnum::ReadSetTooLarge { size, limit: l } if size == limit + 1 && l == limit
    ));
    r.push_list(b"1234")?;
    assert_eq!(r.bytes(), limit);
    Ok(())
}

#[test]
fn test_sealing_sorts_and_drops_duplicates() -> TestResult {
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, 10_000);
    for key in [&b"b"[..], b"a", b"b", b"", b"ab"] {
        r.push_list(key)?;
    }
    for sequence in [3, 1, 3, 2] {
        r.push_absent(id(sequence)?)?;
    }
    r.push_statement(b"k", b"2")?;
    r.push_statement(b"k", b"1")?;
    r.push_statement(b"j", b"9")?;
    r.push_statement(b"k", b"1")?;
    r.push_range(b"m", None)?;
    r.push_range(b"m", Some(b"z"))?;
    r.push_range(b"c", Some(b"d"))?;
    r.push_range(b"m", None)?;
    r.push_range(b"m", Some(b"n"))?;
    assert!(!r.is_sealed());
    r.seal();
    assert!(r.is_sealed());
    let sorted = vec![b"".to_vec(), b"a".to_vec(), b"ab".to_vec(), b"b".to_vec()];
    assert_eq!(lists(&r), sorted);
    assert_eq!(absent(&r), vec![term_key(1), term_key(2), term_key(3)]);
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
    Ok(())
}

#[test]
fn test_before_sealing_the_tables_keep_push_order() -> TestResult {
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, 10_000);
    r.push_list(b"b")?;
    r.push_list(b"a")?;
    r.push_list(b"b")?;
    assert_eq!(lists(&r), vec![b"b".to_vec(), b"a".to_vec(), b"b".to_vec()]);
    Ok(())
}

#[test]
fn test_a_push_after_sealing_unseals() -> TestResult {
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, 10_000);
    r.push_list(b"b")?;
    r.seal();
    r.push_list(b"a")?;
    assert!(!r.is_sealed());
    r.seal();
    assert_eq!(lists(&r), vec![b"a".to_vec(), b"b".to_vec()]);
    Ok(())
}

#[test]
fn test_a_reset_clears_everything_and_unseals() -> TestResult {
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, 10_000);
    r.push_list(b"a")?;
    r.push_statement(b"k", b"o")?;
    r.push_range(b"s", None)?;
    r.push_absent(id(1)?)?;
    r.seal();
    r.reset(4, 9, 500);
    assert!(!r.is_sealed());
    assert_eq!(r.bytes(), CATALOG_BYTES);
    assert!(lists(&r).is_empty() && statements(&r).is_empty());
    assert!(ranges(&r).is_empty() && absent(&r).is_empty());
    assert_eq!((r.tenant(), r.read_version()), (4, 9));
    Ok(())
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

impl Model {
    /// Records the read a push of `kind` made: a list, a statement, a bounded range, an absence or an open range.
    fn record(&mut self, kind: u64, a: Vec<u8>, b: Vec<u8>) {
        match kind {
            0 => {
                let _new: bool = self.lists.insert(a);
            }
            1 => {
                let _new: bool = self.statements.insert((a, b));
            }
            2 => {
                let _new: bool = self.ranges.insert((a, End::Bounded(b)));
            }
            3 => {
                let _new: bool = self.absent.insert(a);
            }
            _ => {
                let _new: bool = self.ranges.insert((a, End::Open));
            }
        }
    }
}

/// Asserts the sealed read set `r` holds what `model` does, in order.
fn assert_sealed_agrees(r: &ReadSet, model: &Model, seed: u64) {
    assert_eq!(
        lists(r),
        model.lists.iter().cloned().collect::<Vec<_>>(),
        "seed {seed}"
    );
    assert_eq!(
        statements(r),
        model.statements.iter().cloned().collect::<Vec<_>>()
    );
    assert_eq!(absent(r), model.absent.iter().cloned().collect::<Vec<_>>());
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
    assert_eq!(ranges(r), expected, "seed {seed}");
}

#[test]
fn test_the_sealed_read_set_agrees_with_a_set_model_over_random_pushes() -> TestResult {
    let mut r = ReadSet::with_capacity(64, 4);
    for seed in 0..200u64 {
        let mut rng = Rng::new(seed);
        // Small limits so refusals happen, large ones so they do not.
        let limit = CATALOG_BYTES + rng.below(400) as usize;
        let tenant = rng.below(4);
        r.reset(tenant, seed, limit);
        let mut model = Model {
            bytes: CATALOG_BYTES,
            ..Model::default()
        };
        for _ in 0..rng.below(60) {
            let kind = rng.below(5);
            let (mut a, b) = (rng.key(6), rng.key(6));
            let sequence = 1 + rng.below(5);
            if kind == 3 {
                a = term_key(sequence);
            }
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
                3 => r.push_absent(id(sequence)?),
                _ => r.push_range(&a, None),
            };
            if model.bytes + cost > limit {
                assert_eq!(
                    result,
                    Err(TxnError::read_set_too_large(model.bytes + cost, limit))
                );
            } else {
                assert_eq!(result, Ok(()), "seed {seed}");
                model.bytes += cost;
                model.record(kind, a, b);
            }
            assert_eq!(r.bytes(), model.bytes, "seed {seed}");
        }
        r.seal();
        assert_eq!(r.tenant(), tenant);
        assert_sealed_agrees(&r, &model, seed);
    }
    Ok(())
}

#[test]
fn test_a_reused_read_set_keeps_its_capacity_and_grows_no_more_for_what_fits() -> TestResult {
    let mut r = ReadSet::new();
    let fill = |set: &mut ReadSet| -> TestResult {
        set.reset(TENANT, 1, 1 << 20);
        for i in 0u32..100 {
            set.push_list(&i.to_be_bytes())?;
            set.push_statement(&i.to_be_bytes(), b"object")?;
            set.push_range(&i.to_be_bytes(), None)?;
            set.push_absent(id(u64::from(i) + 1)?)?;
        }
        set.seal();
        Ok(())
    };
    fill(&mut r)?;
    let grown = r.capacity();
    assert!(
        grown.0 >= 100 * (4 + 4 + 6 + 4 + 17) && grown.1 >= 400,
        "{grown:?}"
    );
    for _ in 0..3 {
        r.reset(TENANT, 2, 1 << 20);
        assert_eq!(r.capacity(), grown, "a reset keeps the buffers");
        fill(&mut r)?;
        assert_eq!(r.capacity(), grown, "what fits takes no more room");
    }
    Ok(())
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
fn test_the_capacity_is_the_arenas_and_the_sum_of_the_four_tables() {
    let r = ReadSet::with_capacity(4_096, 32);
    // std's own vectors reserved alike: the arena of bytes, and four tables of 32 entries each, a list's and an absent
    // key's span (8 bytes), a statement's two (16) and a range's start and optional end (20).
    let arena = Vec::<u8>::with_capacity(4_096).capacity();
    let tables = Vec::<[u8; 8]>::with_capacity(32).capacity()
        + Vec::<[u8; 16]>::with_capacity(32).capacity()
        + Vec::<[u8; 20]>::with_capacity(32).capacity()
        + Vec::<[u8; 8]>::with_capacity(32).capacity();
    assert_eq!(r.capacity(), (arena, tables));
}

#[test]
fn test_gathering_an_executions_reads_records_each_of_them() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "z")])?;
    let executed = applier
        .execute(&changeset(vec![
            insert("a", "knows", "b"),
            insert("c", "likes", "d"),
        ]))
        .context("executed")?;
    let reads = executed.reads();
    assert!(!reads.statements().is_empty() && !reads.absent().is_empty());
    let mut r = ReadSet::new();
    r.reset(TENANT, executed.read_ts(), 1 << 20);
    r.gather(reads)?;
    // Gathering accounts as pushing each read one at a time does: a second path through the documented rule.
    let mut oracle = pushed(reads, TENANT, executed.read_ts())?;
    assert_eq!(r.bytes(), oracle.bytes());
    assert!(r.bytes() > CATALOG_BYTES, "the reads are accounted");
    r.seal();
    oracle.seal();
    assert_eq!(lists(&r), lists(&oracle));
    assert_eq!(statements(&r), statements(&oracle));
    assert_eq!(ranges(&r), ranges(&oracle));
    assert_eq!(absent(&r), absent(&oracle));
    assert_eq!(lists(&r), reads.lists().to_vec());
    assert_eq!(statements(&r), reads.statements().to_vec());
    assert_eq!(ranges(&r), reads.ranges().to_vec());
    let absent_keys: Vec<Vec<u8>> = reads
        .absent()
        .iter()
        .map(|id| ReadSet::term_key(*id).to_vec())
        .collect();
    assert_eq!(absent(&r), absent_keys);
    // The bound exactly, and one byte below it: the gather is checked as a whole before anything is added, its
    // statements counted with two spans each.
    let needed = oracle.bytes();
    let mut at = ReadSet::new();
    at.reset(TENANT, executed.read_ts(), needed);
    assert_eq!(at.gather(reads), Ok(()));
    assert_eq!(at.bytes(), needed);
    let mut below = ReadSet::new();
    below.reset(TENANT, executed.read_ts(), needed - 1);
    assert_eq!(
        below.gather(reads),
        Err(TxnError::read_set_too_large(needed, needed - 1))
    );
    assert_eq!(below.bytes(), CATALOG_BYTES, "nothing is added");
    Ok(())
}

/// `reads` recorded one push at a time into a read set of `tenant` at `version` with no effective bound: the oracle
/// `gather`, which reserves room for all of them at once, is checked against.
fn pushed(
    reads: &ultragraphdb_store_apply::Reads,
    tenant: u64,
    version: u64,
) -> TestResult<ReadSet> {
    let mut r = ReadSet::new();
    r.reset(tenant, version, usize::MAX);
    for key in reads.lists() {
        r.push_list(key)?;
    }
    for (key, object) in reads.statements() {
        r.push_statement(key, object)?;
    }
    for (start, end) in reads.ranges() {
        r.push_range(start, end.as_deref())?;
    }
    for id in reads.absent() {
        r.push_absent(*id)?;
    }
    Ok(r)
}

#[test]
fn test_gathering_a_declarations_scan_records_its_ranges() -> TestResult {
    // Narrowing a predicate that holds data scans its lists: the execution's reads hold ranges, and so must the set.
    let applier = applier()?;
    store(&applier, vec![insert_literal("a", "nick", "x")])?;
    let single = PredicateDeclaration::new(ex("nick")).with_cardinality(Cardinality::Single);
    let executed = applier
        .execute(&changeset(vec![Change::Declare(Declaration::Predicate(
            single,
        ))]))
        .context("executed")?;
    let reads = executed.reads();
    assert!(
        !reads.ranges().is_empty(),
        "the declaration scans its predicate's data"
    );
    let mut r = ReadSet::new();
    r.reset(TENANT, executed.read_ts(), 1 << 20);
    r.gather(reads)?;
    r.seal();
    let gathered: BTreeSet<_> = ranges(&r).into_iter().collect();
    let expected: BTreeSet<_> = reads.ranges().iter().cloned().collect();
    assert_eq!(gathered, expected);
    // A range names two keys, its end included: accounted as pushing each range does.
    let needed = pushed(reads, TENANT, executed.read_ts())?.bytes();
    assert_eq!(r.bytes(), needed);
    // The bound exactly, and one byte below it: each range's start and end are counted as their bytes.
    let mut at = ReadSet::new();
    at.reset(TENANT, executed.read_ts(), needed);
    assert_eq!(at.gather(reads), Ok(()));
    let mut below = ReadSet::new();
    below.reset(TENANT, executed.read_ts(), needed - 1);
    assert_eq!(
        below.gather(reads),
        Err(TxnError::read_set_too_large(needed, needed - 1))
    );
    Ok(())
}

#[test]
fn test_a_limit_past_u32_is_capped_where_spans_reach() {
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, usize::MAX);
    assert_eq!(r.limit(), u32::MAX as usize);
}

#[test]
fn test_a_gather_past_the_limit_leaves_the_read_set_as_it_was() -> TestResult {
    let applier = applier()?;
    let executed = applier
        .execute(&changeset(vec![insert("a", "knows", "b")]))
        .context("executed")?;
    let mut r = ReadSet::new();
    r.reset(TENANT, 1, CATALOG_BYTES + 20);
    r.push_list(b"before")?;
    let error = r.gather(executed.reads()).err().context("the call fails")?;
    // The size refused: the 14 bytes of "before" and its span, and everything the reads add, as pushing them would.
    let added = pushed(executed.reads(), TENANT, 1)?.bytes() - CATALOG_BYTES;
    assert!(added > 6, "the reads take more than the 6 bytes left");
    assert_eq!(
        error,
        TxnError::read_set_too_large(CATALOG_BYTES + 14 + added, CATALOG_BYTES + 20)
    );
    assert_eq!(r.bytes(), CATALOG_BYTES + 14);
    assert_eq!(lists(&r), vec![b"before".to_vec()]);
    assert!(statements(&r).is_empty() && absent(&r).is_empty());
    Ok(())
}
