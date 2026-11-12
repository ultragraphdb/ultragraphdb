/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The per-run key-range skip (approved 2026-10-28): a check skips a run whose keys, or whose written ranges, cannot
//! hold a read's key, and decides nothing by it. Each test reads at a run's bounds, inside them where nothing was
//! written, and past them, alone and after merges, which is where an off-by-one at a bound, a bound not carried by a
//! merge or a skip that decides would show.

use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transactions_commit::ConflictHistory;

use crate::utils::proposal_double::{Reads, Writes, read_set, term_key};

fn insert(history: &mut ConflictHistory, version: u64, writes: &Writes) -> TestResult {
    Ok(history.insert(
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
    )?)
}

fn points(keys: &[&[u8]]) -> Writes {
    Writes {
        points: keys.iter().map(|k| k.to_vec()).collect(),
        ..Writes::default()
    }
}

fn lists(keys: &[&[u8]]) -> Reads {
    Reads {
        lists: keys.iter().map(|k| k.to_vec()).collect(),
        ..Reads::default()
    }
}

/// A history of one run holding the points `d` and `m`, written at 12.
fn one_run() -> TestResult<ConflictHistory> {
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &points(&[b"d", b"m"]))?;
    Ok(history)
}

#[test]
fn test_a_key_at_either_bound_of_a_run_conflicts_and_one_past_it_does_not() -> TestResult {
    let history = one_run()?;
    let check = |reads: &Reads| read_set(0, 10, reads).map(|set| history.conflict(&set));
    assert_eq!(check(&Reads::list(b"d"))?, Some(ConflictKind::List));
    assert_eq!(check(&Reads::list(b"m"))?, Some(ConflictKind::List));
    assert_eq!(check(&Reads::list(b"c\xff"))?, None);
    assert_eq!(check(&Reads::list(b"m\x00"))?, None);
    // Inside the bounds, written by nothing: the skip does not decide.
    assert_eq!(check(&Reads::list(b"g"))?, None);
    // Keys below and above the bounds around the one that conflicts, in order.
    assert_eq!(
        check(&lists(&[b"a", b"m", b"z"]))?,
        Some(ConflictKind::List)
    );
    assert_eq!(check(&lists(&[b"a", b"b", b"z"]))?, None);
    // A run holding the term key of `id(3)` among its points.
    let mut terms = ConflictHistory::new(0);
    let term = term_key(3);
    insert(&mut terms, 12, &points(&[b"d", &term]))?;
    let absent =
        |sequence| read_set(0, 10, &Reads::absent(sequence)).map(|set| terms.conflict(&set));
    assert_eq!(absent(3)?, Some(ConflictKind::AbsentTerm));
    assert_eq!(absent(4)?, None);
    Ok(())
}

#[test]
fn test_a_statement_at_a_bound_conflicts() -> TestResult {
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &Writes::statement(b"m", b"o"))?;
    insert(&mut history, 14, &Writes::statement(b"d", b"o"))?;
    let check = |reads: &Reads| read_set(0, 10, reads).map(|set| history.conflict(&set));
    assert_eq!(
        check(&Reads::statement(b"m", b"o"))?,
        Some(ConflictKind::Statement)
    );
    assert_eq!(
        check(&Reads::statement(b"d", b"o"))?,
        Some(ConflictKind::Statement)
    );
    assert_eq!(check(&Reads::statement(b"d", b"p"))?, None);
    assert_eq!(check(&Reads::statement(b"n", b"o"))?, None);
    Ok(())
}

#[test]
fn test_a_range_read_meeting_a_run_at_a_bound_conflicts() -> TestResult {
    let history = one_run()?;
    let check = |reads: &Reads| read_set(0, 10, reads).map(|set| history.conflict(&set));
    // Ending at the lower bound, the end exclusive: no key of the run inside.
    assert_eq!(check(&Reads::range(b"a", Some(b"d")))?, None);
    assert_eq!(
        check(&Reads::range(b"a", Some(b"d\x00")))?,
        Some(ConflictKind::Range)
    );
    assert_eq!(
        check(&Reads::range(b"m", Some(b"z")))?,
        Some(ConflictKind::Range)
    );
    assert_eq!(check(&Reads::range(b"m\x00", None))?, None);
    assert_eq!(check(&Reads::range(b"e", Some(b"m")))?, None);
    assert_eq!(check(&Reads::range(b"a", None))?, Some(ConflictKind::Range));
    Ok(())
}

#[test]
fn test_a_merged_run_keeps_both_runs_bounds() -> TestResult {
    // Two runs of one point each merge into one (the newest is no larger than the one before).
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &points(&[b"m"]))?;
    insert(&mut history, 14, &points(&[b"d"]))?;
    assert_eq!(history.runs(), 1, "merged");
    let check = |reads: &Reads| read_set(0, 10, reads).map(|set| history.conflict(&set));
    assert_eq!(check(&Reads::list(b"d"))?, Some(ConflictKind::List));
    assert_eq!(check(&Reads::list(b"m"))?, Some(ConflictKind::List));
    assert_eq!(check(&Reads::list(b"g"))?, None);
    Ok(())
}

#[test]
fn test_a_written_range_at_its_bounds_conflicts_alone_and_after_a_merge() -> TestResult {
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &Writes::range(b"m", Some(b"p")))?;
    let check = |checked: &ConflictHistory, reads: &Reads| {
        read_set(0, 10, reads).map(|set| checked.conflict(&set))
    };
    assert_eq!(
        check(&history, &Reads::list(b"m"))?,
        Some(ConflictKind::Drop)
    );
    assert_eq!(
        check(&history, &Reads::list(b"o\xff"))?,
        Some(ConflictKind::Drop)
    );
    assert_eq!(check(&history, &Reads::list(b"p"))?, None);
    assert_eq!(check(&history, &Reads::list(b"l\xff"))?, None);
    // A second range, below the first, merged into its run: the merged run's ranges reach from the lower start to
    // the higher end, and then to no end at all.
    insert(&mut history, 14, &Writes::range(b"b", Some(b"c")))?;
    assert_eq!(history.runs(), 1, "merged");
    assert_eq!(
        check(&history, &Reads::list(b"b"))?,
        Some(ConflictKind::Drop)
    );
    assert_eq!(
        check(&history, &Reads::list(b"o"))?,
        Some(ConflictKind::Drop)
    );
    assert_eq!(
        check(&history, &Reads::list(b"g"))?,
        None,
        "between the two ranges"
    );
    assert_eq!(check(&history, &Reads::list(b"a"))?, None);
    insert(&mut history, 16, &Writes::range(b"x", None))?;
    insert(&mut history, 18, &Writes::range(b"y", Some(b"z")))?;
    assert_eq!(
        check(&history, &Reads::list(b"\xff\xff"))?,
        Some(ConflictKind::Drop)
    );
    assert_eq!(
        check(&history, &Reads::range(b"q", Some(b"r")))?,
        None,
        "between the ranges"
    );
    assert_eq!(
        check(&history, &Reads::range(b"q", Some(b"x\x00")))?,
        Some(ConflictKind::Drop)
    );
    Ok(())
}

#[test]
fn test_a_written_range_over_the_lowest_keys_conflicts_at_them() -> TestResult {
    // A range from the empty key to 0x00 0x05: the empty key and 0x00 0x01 lie inside it, 0x00 0x05 is its end.
    let mut history = ConflictHistory::new(0);
    insert(&mut history, 12, &Writes::range(b"", Some(b"\x00\x05")))?;
    let check = |reads: &Reads| read_set(0, 10, reads).map(|set| history.conflict(&set));
    assert_eq!(check(&Reads::list(b""))?, Some(ConflictKind::Drop));
    assert_eq!(check(&Reads::list(b"\x00\x01"))?, Some(ConflictKind::Drop));
    assert_eq!(check(&Reads::list(b"\x00\x05"))?, None);
    Ok(())
}
