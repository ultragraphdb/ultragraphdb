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
    board.close(A.0);
    assert_eq!(
        board.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        [OTHER]
    );
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
