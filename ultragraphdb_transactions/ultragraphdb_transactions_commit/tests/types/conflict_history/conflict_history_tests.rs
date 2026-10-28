/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_transactions_commit::ConflictHistory;

use crate::utils::proposal_double::{Reads, Writes, read_set};

/// Inserts `writes` at `version`.
fn insert(history: &mut ConflictHistory, version: u64, writes: &Writes) {
    history.insert(
        version,
        writes.points.iter().map(Vec::as_slice),
        writes
            .statements
            .iter()
            .map(|(k, o)| (k.as_slice(), o.as_slice())),
        writes
            .ranges
            .iter()
            .map(|(s, e)| (s.as_slice(), e.as_deref())),
    );
}

/// What a read of `reads` at read version 10 conflicts with after `writes` committed at 12.
fn after(writes: &Writes, reads: &Reads) -> Option<ConflictKind> {
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, writes);
    history.conflict(&read_set(0, 10, reads))
}

#[test]
fn test_a_list_read_whole_conflicts_with_its_key_its_statements_and_a_range_over_it() {
    let read = Reads::list(b"k");
    assert_eq!(after(&Writes::point(b"k"), &read), Some(ConflictKind::List));
    assert_eq!(
        after(&Writes::statement(b"k", b"o"), &read),
        Some(ConflictKind::List)
    );
    assert_eq!(
        after(&Writes::range(b"j", Some(b"l")), &read),
        Some(ConflictKind::Drop)
    );
    assert_eq!(
        after(&Writes::range(b"a", None), &read),
        Some(ConflictKind::Drop)
    );
    assert_eq!(after(&Writes::point(b"kk"), &read), None);
    assert_eq!(after(&Writes::point(b""), &read), None);
    assert_eq!(after(&Writes::statement(b"kk", b"o"), &read), None);
    assert_eq!(after(&Writes::range(b"l", None), &read), None);
    assert_eq!(after(&Writes::range(b"j", Some(b"k")), &read), None);
}

#[test]
fn test_a_statement_tested_conflicts_with_its_list_written_whole_and_with_itself() {
    let read = Reads::statement(b"k", b"o");
    assert_eq!(
        after(&Writes::point(b"k"), &read),
        Some(ConflictKind::Statement)
    );
    assert_eq!(
        after(&Writes::statement(b"k", b"o"), &read),
        Some(ConflictKind::Statement)
    );
    assert_eq!(
        after(&Writes::range(b"k", Some(b"l")), &read),
        Some(ConflictKind::Drop)
    );
    // The spec's "Inserts of different objects into one list both commit": another object of the list does not.
    assert_eq!(after(&Writes::statement(b"k", b"p"), &read), None);
    assert_eq!(after(&Writes::statement(b"j", b"o"), &read), None);
    assert_eq!(after(&Writes::point(b"o"), &read), None);
}

#[test]
fn test_a_scanned_range_conflicts_with_keys_inside_it_and_ranges_over_it() {
    let read = Reads::range(b"b", Some(b"d"));
    assert_eq!(
        after(&Writes::point(b"b"), &read),
        Some(ConflictKind::Range)
    );
    assert_eq!(
        after(&Writes::point(b"c"), &read),
        Some(ConflictKind::Range)
    );
    assert_eq!(
        after(&Writes::point(b"czzz"), &read),
        Some(ConflictKind::Range)
    );
    assert_eq!(
        after(&Writes::statement(b"c", b"o"), &read),
        Some(ConflictKind::Range)
    );
    assert_eq!(
        after(&Writes::range(b"c", Some(b"e")), &read),
        Some(ConflictKind::Drop)
    );
    assert_eq!(
        after(&Writes::range(b"a", Some(b"bb")), &read),
        Some(ConflictKind::Drop)
    );
    // The end is exclusive, the start inclusive.
    assert_eq!(after(&Writes::point(b"d"), &read), None);
    assert_eq!(after(&Writes::point(b"a"), &read), None);
    assert_eq!(after(&Writes::statement(b"d", b"o"), &read), None);
    assert_eq!(after(&Writes::range(b"d", Some(b"e")), &read), None);
    assert_eq!(after(&Writes::range(b"a", Some(b"b")), &read), None);
    // An open end reaches every later key.
    let open = Reads::range(b"b", None);
    assert_eq!(
        after(&Writes::point(b"zz"), &open),
        Some(ConflictKind::Range)
    );
    assert_eq!(
        after(&Writes::range(b"zz", None), &open),
        Some(ConflictKind::Drop)
    );
    assert_eq!(after(&Writes::point(b"a"), &open), None);
}

#[test]
fn test_an_empty_range_conflicts_with_nothing() {
    let read = Reads::range(b"d", Some(b"b"));
    assert_eq!(after(&Writes::point(b"c"), &read), None);
    assert_eq!(after(&Writes::range(b"a", None), &read), None);
}

#[test]
fn test_a_key_found_absent_conflicts_with_its_creation() {
    let read = Reads::absent(b"iri");
    assert_eq!(
        after(&Writes::point(b"iri"), &read),
        Some(ConflictKind::AbsentTerm)
    );
    assert_eq!(
        after(&Writes::range(b"i", None), &read),
        Some(ConflictKind::Drop)
    );
    assert_eq!(after(&Writes::point(b"iri2"), &read), None);
}

#[test]
fn test_the_catalog_range_conflicts_with_a_catalog_write() {
    let catalog = read_set(0, 10, &Reads::default());
    let mut key = catalog.catalog().0.to_vec();
    key.extend_from_slice(b"score");
    assert_eq!(
        after(&Writes::point(&key), &Reads::default()),
        Some(ConflictKind::Catalog)
    );
    // A key of another namespace's catalog is outside it.
    let other = read_set(1, 10, &Reads::default());
    let mut key = other.catalog().0.to_vec();
    key.extend_from_slice(b"score");
    assert_eq!(after(&Writes::point(&key), &Reads::default()), None);
}

#[test]
fn test_kinds_are_reported_in_order_and_ranges_last() {
    let reads = Reads {
        lists: vec![b"a".to_vec()],
        absent: vec![b"b".to_vec()],
        statements: vec![(b"s".to_vec(), b"o".to_vec())],
        ..Reads::default()
    };
    let both = Writes {
        points: vec![b"b".to_vec(), b"a".to_vec()],
        ..Writes::default()
    };
    assert_eq!(after(&both, &reads), Some(ConflictKind::List));
    assert_eq!(
        after(&Writes::point(b"b"), &reads),
        Some(ConflictKind::AbsentTerm)
    );
    let dropped = Writes {
        ranges: vec![(b"a".to_vec(), Some(b"aa".to_vec()))],
        points: vec![b"s".to_vec()],
        ..Writes::default()
    };
    assert_eq!(after(&dropped, &reads), Some(ConflictKind::Statement));
}

#[test]
fn test_only_writes_above_the_read_version_conflict() {
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 10, &Writes::point(b"k"));
    let read = Reads::list(b"k");
    assert_eq!(history.conflict(&read_set(0, 10, &read)), None);
    assert_eq!(history.conflict(&read_set(0, 11, &read)), None);
    assert_eq!(
        history.conflict(&read_set(0, 9, &read)),
        Some(ConflictKind::List)
    );
    assert_eq!(
        history.conflict(&read_set(0, 0, &read)),
        Some(ConflictKind::List)
    );
}

#[test]
fn test_an_empty_history_and_an_empty_read_set_conflict_with_nothing() {
    let history = ConflictHistory::new(0);
    assert!(history.is_empty());
    assert_eq!(history.conflict(&read_set(0, 0, &Reads::list(b"k"))), None);
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &Writes::point(b"k"));
    assert_eq!(history.conflict(&read_set(0, 0, &Reads::default())), None);
}

#[test]
fn test_the_newest_write_of_a_key_decides() {
    // A key written at 10 and again at 20, in transactions far enough apart to sit in separate runs or merged.
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 10, &Writes::point(b"k"));
    for version in (12..20).step_by(2) {
        insert(&mut history, version, &Writes::point(b"other"));
    }
    insert(&mut history, 20, &Writes::point(b"k"));
    assert_eq!(
        history.conflict(&read_set(0, 18, &Reads::list(b"k"))),
        Some(ConflictKind::List)
    );
    assert_eq!(history.conflict(&read_set(0, 20, &Reads::list(b"k"))), None);
}

#[test]
fn test_pruning_raises_the_floor_and_drops_what_only_older_versions_wrote() {
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 10, &Writes::point(b"a"));
    insert(&mut history, 20, &Writes::point(b"b"));
    insert(&mut history, 30, &Writes::point(b"c"));
    let before = history.bytes();
    assert_eq!(history.floor(), 0);
    history.prune(20);
    assert_eq!(history.floor(), 20);
    assert!(history.bytes() < before, "{} < {before}", history.bytes());
    let reads = Reads {
        lists: vec![b"b".to_vec(), b"c".to_vec()],
        ..Reads::default()
    };
    assert_eq!(
        history.conflict(&read_set(0, 20, &reads)),
        Some(ConflictKind::List)
    );
    assert_eq!(history.conflict(&read_set(0, 20, &Reads::list(b"b"))), None);
    // The floor never goes down.
    history.prune(15);
    assert_eq!(history.floor(), 20);
    // Past every version, nothing is held.
    history.prune(30);
    assert!(history.is_empty());
    assert_eq!(history.bytes(), 0);
    assert_eq!(history.runs(), 0);
}

#[test]
fn test_a_history_opened_at_a_floor_keeps_it() {
    assert_eq!(ConflictHistory::new(100).floor(), 100);
}

#[test]
fn test_the_bytes_count_the_keys_and_24_per_entry() {
    let mut history = ConflictHistory::new(0);
    assert_eq!(history.bytes(), 0);
    insert(&mut history, 2, &Writes::point(b"abc"));
    // 3 bytes of key and one entry.
    assert_eq!(history.bytes(), 27);
    assert_eq!(history.len(), 1);
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 2, &Writes::statement(b"k", b"o"));
    // Two bytes of key and object, the statement and its list touched.
    assert_eq!(history.bytes(), 50);
    assert_eq!(history.len(), 2);
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 2, &Writes::range(b"a", None));
    assert_eq!(history.bytes(), 25);
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 2, &Writes::range(b"a", Some(b"bc")));
    assert_eq!(history.bytes(), 27);
}

#[test]
fn test_a_key_written_twice_is_held_once() {
    let mut history = ConflictHistory::new(0);
    let twice = Writes {
        points: vec![b"k".to_vec(), b"k".to_vec()],
        statements: vec![
            (b"s".to_vec(), b"o".to_vec()),
            (b"s".to_vec(), b"o".to_vec()),
        ],
        ..Writes::default()
    };
    insert(&mut history, 2, &twice);
    // `k`, `s` touched by its statement, and the statement.
    assert_eq!(history.len(), 3);
    insert(&mut history, 4, &twice);
    // The second transaction's run merges into the first's, which is no larger, and each key is held once.
    assert_eq!(history.runs(), 1);
    assert_eq!(history.len(), 3);
    assert_eq!(
        history.conflict(&read_set(0, 2, &Reads::list(b"k"))),
        Some(ConflictKind::List)
    );
}

#[test]
fn test_runs_merge_to_a_logarithmic_number() {
    let mut history = ConflictHistory::new(0);
    for i in 0..1_000u64 {
        insert(&mut history, 2 * (i + 1), &Writes::point(&i.to_be_bytes()));
    }
    // A binary counter over 1,000 single-entry runs holds at most one run per bit: 10.
    assert!(history.runs() <= 10, "{}", history.runs());
    assert_eq!(history.len(), 1_000);
}

#[test]
fn test_a_merge_stops_at_the_run_cap() {
    let cap = ConflictHistory::RUN_CAP as u64;
    let mut history = ConflictHistory::new(0);
    for i in 0..3 * cap {
        insert(&mut history, 2 * (i + 1), &Writes::point(&i.to_be_bytes()));
    }
    assert_eq!(history.len(), 3 * cap as usize);
    // At least three runs, since none holds more than the cap; at most those and one per bit of the cap below them.
    assert!(history.runs() >= 3, "{}", history.runs());
    assert!(history.runs() <= 3 + 15, "{}", history.runs());
    assert_eq!(
        history.conflict(&read_set(0, 0, &Reads::list(&0u64.to_be_bytes()))),
        Some(ConflictKind::List)
    );
}

#[test]
fn test_a_steady_state_allocates_nothing_more() {
    // Rounds of the same shape, each pruned below the one before: once warm, the buffers are reused.
    let mut history = ConflictHistory::new(0);
    let mut version = 2;
    let cycle = |history: &mut ConflictHistory, version: &mut u64| {
        for t in 0..32u64 {
            let writes = Writes {
                points: vec![t.to_be_bytes().to_vec(), (t + 100).to_be_bytes().to_vec()],
                statements: vec![(b"list".to_vec(), t.to_be_bytes().to_vec())],
                ..Writes::default()
            };
            insert(history, *version, &writes);
            *version += 2;
        }
        history.prune(*version - 2 * 32);
    };
    for _ in 0..200 {
        cycle(&mut history, &mut version);
    }
    let warm = history.capacity();
    for _ in 0..200 {
        cycle(&mut history, &mut version);
    }
    assert_eq!(history.capacity(), warm);
    assert!(history.capacity() >= history.bytes());
}

#[test]
fn test_a_list_written_whole_and_by_statements_in_one_transaction_is_held_as_both() {
    // Forty lists, each written whole and by a statement in the same transaction: a statement of another object
    // conflicts through the whole write, and a list read through either.
    let keys: Vec<Vec<u8>> = (0..40u8).map(|i| vec![b'k', i]).collect();
    let writes = Writes {
        points: keys.clone(),
        statements: keys.iter().map(|k| (k.clone(), b"o".to_vec())).collect(),
        ..Writes::default()
    };
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &writes);
    for key in &keys {
        assert_eq!(
            history.conflict(&read_set(0, 10, &Reads::statement(key, b"p"))),
            Some(ConflictKind::Statement),
            "{key:?}"
        );
        assert_eq!(
            history.conflict(&read_set(0, 10, &Reads::list(key))),
            Some(ConflictKind::List)
        );
    }
}

#[test]
fn test_a_range_written_over_the_catalog_conflicts_as_a_drop() {
    // A namespace's drop writes its whole catalog range, which every read set of the namespace holds.
    let rs = read_set(0, 10, &Reads::default());
    let (start, end) = rs.catalog();
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &Writes::range(start, end));
    assert_eq!(history.conflict(&rs), Some(ConflictKind::Drop));
    let other = read_set(1, 10, &Reads::default());
    assert_eq!(history.conflict(&other), None);
}

#[test]
fn test_a_merge_leaves_out_what_is_at_or_below_the_floor() {
    // a at 2 and b at 4 merge; the floor rises to 2; c at 6 and d at 8 merge, then with the run of a and b, which
    // drops a.
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 2, &Writes::point(b"a"));
    insert(&mut history, 4, &Writes::point(b"b"));
    history.prune(2);
    assert_eq!(history.len(), 2);
    insert(&mut history, 6, &Writes::point(b"c"));
    insert(&mut history, 8, &Writes::point(b"d"));
    assert_eq!(history.runs(), 1);
    assert_eq!(history.len(), 3);
}

#[test]
fn test_pruning_keeps_only_a_bounded_number_of_runs_for_reuse() {
    // 511 single-entry runs merge as a binary counter into nine runs; pruning them all keeps no more for reuse than
    // the pool holds, so the capacity falls.
    let mut history = ConflictHistory::new(0);
    for i in 0..511u64 {
        insert(&mut history, 2 * (i + 1), &Writes::point(&i.to_be_bytes()));
    }
    assert_eq!(history.runs(), 9);
    let before = history.capacity();
    history.prune(2 * 511);
    assert_eq!(history.runs(), 0);
    assert!(
        history.capacity() < before,
        "{} < {before}",
        history.capacity()
    );
}

#[test]
fn test_a_transaction_that_wrote_nothing_adds_no_run() {
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 2, &Writes::default());
    assert_eq!(history.runs(), 0);
    insert(&mut history, 4, &Writes::point(b"a"));
    insert(&mut history, 6, &Writes::default());
    assert_eq!(history.runs(), 1);
}
