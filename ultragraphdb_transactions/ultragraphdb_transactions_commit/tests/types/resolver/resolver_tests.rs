/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{ConflictKind, WriteSetRecord, WriteSetRecordView};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CommitError, Resolver};
use ultragraphdb_transactions_txn::ReadSet;

use crate::utils::proposal_double::{Reads, Writes, read_set, record};

fn admit(resolver: &mut Resolver, version: u64, writes: &Writes) -> TestResult {
    let bytes = record(0, 0, writes, 0);
    let view = WriteSetRecordView::new(&bytes).context("the record reads")?;
    resolver.admit(version, &view)?;
    Ok(())
}

#[test]
fn test_an_unsealed_read_set_is_refused_first() -> TestResult {
    let resolver = Resolver::new(20);
    let mut rs = ReadSet::new();
    rs.reset(0, 10, usize::MAX);
    rs.push_list(b"k").context("within the limit")?;
    assert_eq!(resolver.check(&rs), Err(CommitError::unsealed()));
    Ok(())
}

#[test]
fn test_a_read_version_below_the_floor_is_too_old_before_any_conflict() -> TestResult {
    let mut resolver = Resolver::new(20);
    admit(&mut resolver, 22, &Writes::point(b"k"))?;
    assert_eq!(
        resolver.check(&read_set(0, 19, &Reads::list(b"k"))?),
        Err(CommitError::too_old(19, 20))
    );
    assert_eq!(
        resolver.check(&read_set(0, 20, &Reads::list(b"k"))?),
        Err(CommitError::conflict(ConflictKind::List))
    );
    assert_eq!(
        resolver.check(&read_set(0, 20, &Reads::list(b"j"))?),
        Ok(())
    );
    Ok(())
}

#[test]
fn test_a_check_changes_nothing() -> TestResult {
    let mut resolver = Resolver::new(0);
    admit(&mut resolver, 12, &Writes::point(b"k"))?;
    let rs = read_set(0, 10, &Reads::list(b"k"))?;
    let bytes = resolver.history().bytes();
    assert_eq!(
        resolver.check(&rs),
        Err(CommitError::conflict(ConflictKind::List))
    );
    assert_eq!(
        resolver.check(&rs),
        Err(CommitError::conflict(ConflictKind::List))
    );
    assert_eq!(resolver.history().bytes(), bytes);
    // A transaction that only checks adds no writes: a read of what it would have written does not conflict.
    let mut checking = Resolver::new(0);
    assert_eq!(checking.check(&read_set(0, 10, &Reads::default())?), Ok(()));
    assert!(checking.history().is_empty());
    admit(&mut checking, 12, &Writes::default())?;
    assert_eq!(
        checking.check(&read_set(0, 10, &Reads::list(b"k"))?),
        Ok(())
    );
    Ok(())
}

#[test]
fn test_admitting_adds_every_conflict_section_of_the_record() -> TestResult {
    let writes = Writes {
        points: vec![b"p".to_vec()],
        statements: vec![(b"s".to_vec(), b"o".to_vec())],
        ranges: vec![(b"m".to_vec(), Some(b"n".to_vec()))],
    };
    let mut resolver = Resolver::new(0);
    admit(&mut resolver, 12, &writes)?;
    let at = |reads: &Reads| read_set(0, 10, reads).map(|set| resolver.check(&set));
    assert_eq!(
        at(&Reads::list(b"p"))?,
        Err(CommitError::conflict(ConflictKind::List))
    );
    assert_eq!(
        at(&Reads::statement(b"s", b"o"))?,
        Err(CommitError::conflict(ConflictKind::Statement))
    );
    assert_eq!(
        at(&Reads::list(b"mm"))?,
        Err(CommitError::conflict(ConflictKind::Drop))
    );
    assert_eq!(at(&Reads::list(b"n"))?, Ok(()));
    Ok(())
}

#[test]
fn test_a_record_range_with_an_empty_end_reaches_past_the_last_key() -> TestResult {
    let mut resolver = Resolver::new(0);
    admit(&mut resolver, 12, &Writes::range(b"m", None))?;
    assert_eq!(
        resolver.check(&read_set(0, 10, &Reads::list(b"zzz"))?),
        Err(CommitError::conflict(ConflictKind::Drop))
    );
    assert_eq!(
        resolver.check(&read_set(0, 10, &Reads::list(b"l"))?),
        Ok(())
    );
    Ok(())
}

#[test]
fn test_the_record_list_deltas_and_dictionary_alone_are_not_conflict_keys() -> TestResult {
    // Only the conflict sections are the history's: a delta's key the worker left out of them conflicts with nothing.
    let mut record = WriteSetRecord::transaction(0);
    record.push_inserted(b"k".to_vec(), None);
    record.push_statement_record(b"d".to_vec(), Some(vec![2]));
    let bytes = record.encode();
    let view = WriteSetRecordView::new(&bytes).context("the record reads")?;
    let mut resolver = Resolver::new(0);
    resolver.admit(12, &view)?;
    assert_eq!(
        resolver.check(&read_set(0, 10, &Reads::list(b"k"))?),
        Ok(())
    );
    assert_eq!(resolver.check(&read_set(0, 10, &Reads::absent(1))?), Ok(()));
    Ok(())
}

#[test]
fn test_two_first_statements_of_one_term_conflict_on_its_absence() -> TestResult {
    // Both executions found the term absent and both write a statement of it; the second to be decided conflicts,
    // whether the first was admitted in its round or committed above its read version.
    let statement = Writes::term(5);
    let mut resolver = Resolver::new(0);
    let first = read_set(0, 10, &Reads::absent(5))?;
    assert_eq!(resolver.check(&first), Ok(()));
    admit(&mut resolver, 12, &statement)?;
    let second = read_set(0, 10, &Reads::absent(5))?;
    assert_eq!(
        resolver.check(&second),
        Err(CommitError::conflict(ConflictKind::AbsentTerm))
    );
    // Another term is free.
    assert_eq!(resolver.check(&read_set(0, 10, &Reads::absent(6))?), Ok(()));
    Ok(())
}

#[test]
fn test_pruning_raises_the_floor() -> TestResult {
    let mut resolver = Resolver::new(4);
    assert_eq!(resolver.history().floor(), 4);
    resolver.prune(10);
    assert_eq!(resolver.history().floor(), 10);
    resolver.prune(6);
    assert_eq!(resolver.history().floor(), 10);
    Ok(())
}

/// A key a transaction writes, whole or as a statement's list, inside a range written above its read version is a
/// write into a dropped range: it conflicts as a drop; at or below its read version, or outside the range, it does not
/// (7.7).
#[test]
fn test_a_write_into_a_range_written_above_the_read_version_conflicts_as_a_drop() -> TestResult {
    let mut resolver = Resolver::new(0);
    admit(&mut resolver, 10, &Writes::range(b"p", Some(b"q")))?;
    admit(&mut resolver, 12, &Writes::range(b"x", None))?;
    let check = |rv: u64, writes: &Writes| -> TestResult<_> {
        let bytes = record(0, rv, writes, 0);
        Ok(resolver.check_writes(rv, &WriteSetRecordView::new(&bytes)?))
    };
    assert_eq!(
        check(8, &Writes::point(b"pa"))?,
        Err(CommitError::conflict(ConflictKind::Drop))
    );
    assert_eq!(
        check(8, &Writes::statement(b"pb", b"o"))?,
        Err(CommitError::conflict(ConflictKind::Drop))
    );
    assert_eq!(
        check(10, &Writes::point(b"pa"))?,
        Ok(()),
        "the drop at the read version"
    );
    assert_eq!(
        check(8, &Writes::point(b"q"))?,
        Ok(()),
        "the range's end is not in it"
    );
    assert_eq!(check(8, &Writes::point(b"o"))?, Ok(()));
    assert_eq!(check(8, &Writes::default())?, Ok(()));
    assert_eq!(
        check(8, &Writes::point(b"zzz"))?,
        Err(CommitError::conflict(ConflictKind::Drop)),
        "an open range reaches past the last key"
    );
    Ok(())
}
