/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::ScanBoard;
use ultragraphdb_tool_bdd::{Context, TestResult};

const A: (u64, u64) = (1, 1);
const B: (u64, u64) = (1, 3);
const OTHER: (u64, u64) = (2, 1);

/// Scans take turns in the order they were given them, and one taken out for its frame is open again once put back.
#[test]
fn test_scans_take_turns_in_order() -> TestResult {
    let mut board = ScanBoard::default();
    board.open(A, "a");
    board.open(B, "b");
    assert!(!board.has_turns(), "opening gives no turn");
    board.give_turn(A);
    board.give_turn(B);
    assert_eq!(board.take_turn(), Some((A, "a")));
    assert_eq!(board.get(A), None, "taken out");
    board.open(A, "a");
    board.give_turn(A);
    assert_eq!(board.take_turn(), Some((B, "b")));
    assert_eq!(board.take_turn(), Some((A, "a")));
    assert_eq!(board.take_turn(), None);
    assert!(board.is_empty());
    Ok(())
}

/// A Scan's end takes its turn with it, so a core with nothing to build does not spin on a turn nothing can take.
#[test]
fn test_a_cancelled_scan_leaves_no_turn() -> TestResult {
    let mut board = ScanBoard::default();
    board.open(A, "a");
    board.give_turn(A);
    assert_eq!(board.remove(A), Some("a"));
    assert!(!board.has_turns());
    assert_eq!(board.remove(A), None);
    Ok(())
}

/// A closing connection takes its Scans' turns with them, and no other's.
#[test]
fn test_a_closed_connection_leaves_no_turn_of_its_own() -> TestResult {
    let mut board = ScanBoard::default();
    for key in [A, OTHER, B] {
        board.open(key, key.1);
        board.give_turn(key);
    }
    board.set_timer(A, 5);
    board.wait_for_epoch(B, 5);
    board.close(A.0);
    assert_eq!(board.get(A), None);
    assert_eq!(board.get(B), None);
    assert_eq!(board.get(OTHER), Some(&OTHER.1));
    assert_eq!(board.next_due(u64::MAX), None, "its time with it");
    assert_eq!(board.next_closed(u64::MAX), None, "its epoch with it");
    assert_eq!(board.take_turn().context("the other's turn")?.0, OTHER);
    assert!(!board.has_turns());
    Ok(())
}

/// A later Scan under the name of one cancelled gets no turn the cancelled one was waiting for: it is served only once it is
/// given its own.
#[test]
fn test_a_scan_reusing_a_name_does_not_inherit_a_turn() -> TestResult {
    let mut board = ScanBoard::default();
    board.open(OTHER, "other");
    board.give_turn(OTHER);
    board.open(A, "cancelled");
    board.give_turn(A);
    let _gone = board.remove(A);
    board.open(A, "waiting");
    assert_eq!(board.take_turn().context("the other's turn")?.0, OTHER);
    assert_eq!(board.take_turn(), None, "it waits on");
    assert_eq!(board.get(A), Some(&"waiting"));
    Ok(())
}

/// A turn goes only to an open Scan.
#[test]
fn test_no_turn_goes_to_a_scan_not_open() {
    let mut board = ScanBoard::<&str>::default();
    board.give_turn(A);
    assert!(!board.has_turns());
}

/// A Scan's time is looked at once it has passed, a later time replacing the earlier, and not again.
#[test]
fn test_a_timer_comes_due_once_at_its_latest_time() {
    let mut board = ScanBoard::default();
    board.open(A, "a");
    board.open(B, "b");
    board.set_timer(A, 10);
    board.set_timer(B, 20);
    board.set_timer(A, 30);
    assert_eq!(board.next_due(19), None);
    assert_eq!(board.next_due(20), Some(B));
    assert_eq!(board.next_due(29), None);
    assert_eq!(board.next_due(30), Some(A));
    assert_eq!(board.next_due(u64::MAX), None);
}

/// A timer cleared, or a Scan ended, comes due no more.
#[test]
fn test_a_timer_ends_with_its_wait() {
    let mut board = ScanBoard::default();
    board.open(A, "a");
    board.open(B, "b");
    board.set_timer(A, 10);
    board.set_timer(B, 10);
    board.clear_timer(A);
    let _gone = board.remove(B);
    assert_eq!(board.next_due(u64::MAX), None);
}

/// A Scan taken for its turn lets go of what it waited on.
#[test]
fn test_a_turn_ends_a_wait() {
    let mut board = ScanBoard::default();
    board.open(A, "a");
    board.set_timer(A, 10);
    board.wait_for_epoch(A, 10);
    board.give_turn(A);
    assert_eq!(board.take_turn(), Some((A, "a")));
    assert_eq!(board.next_due(u64::MAX), None);
    assert_eq!(board.next_closed(u64::MAX), None);
}

/// Scans waiting on epochs come out as those close, and only those.
#[test]
fn test_an_epochs_close_ends_only_its_waits() {
    let mut board = ScanBoard::default();
    board.open(A, "a");
    board.open(B, "b");
    board.wait_for_epoch(A, 7);
    board.wait_for_epoch(B, 9);
    assert_eq!(board.next_closed(6), None);
    assert_eq!(board.next_closed(8), Some(A));
    assert_eq!(board.next_closed(8), None);
    assert_eq!(board.next_closed(9), Some(B));
}

/// A transaction's end names its Scans still open, and no other's, once.
#[test]
fn test_a_transactions_end_names_its_open_scans() {
    const T1: (u64, u64) = (1, 7);
    const T2: (u64, u64) = (1, 8);
    let mut board = ScanBoard::default();
    for key in [A, B, OTHER] {
        board.open(key, key.1);
    }
    board.join_transaction(T1, A);
    board.join_transaction(T1, B);
    board.join_transaction(T2, OTHER);
    let _gone = board.remove(A);
    assert_eq!(board.of_transaction(T1), [B]);
    assert_eq!(board.end_transaction(T1), [B]);
    assert_eq!(board.end_transaction(T1), []);
    assert_eq!(board.of_transaction(T2), [OTHER]);
}

/// The turns waiting count those given and not yet taken.
#[test]
fn test_turns_waiting_counts_the_queue() {
    let mut board = ScanBoard::default();
    board.open(A, "a");
    board.give_turn(A);
    assert_eq!(board.turns_waiting(), 1);
    let _taken = board.take_turn();
    assert_eq!(board.turns_waiting(), 0);
}
