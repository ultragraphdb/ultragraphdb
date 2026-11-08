/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_common_service::InboxTurn;

/// The bound of the turns these tests take.
const MESSAGES_MAX: usize = 4;

/// An inbox holding `count` messages, numbered from 0.
fn inbox(count: usize) -> VecDeque<usize> {
    (0..count).collect()
}

/// The messages one turn takes of `inbox`, in order.
fn one_turn(inbox: &mut VecDeque<usize>) -> Vec<usize> {
    let mut turn = InboxTurn::new(MESSAGES_MAX);
    let mut taken = Vec::new();
    while let Some(message) = turn.receive(|| inbox.pop_front().ok_or("empty")) {
        taken.push(message);
    }
    taken
}

/// A turn takes every message of an inbox holding as many as its bound.
#[test]
fn test_a_turn_takes_an_inbox_at_its_bound_whole() {
    let mut waiting = inbox(MESSAGES_MAX);
    let expected: Vec<usize> = waiting.iter().copied().collect();
    assert_eq!(one_turn(&mut waiting), expected);
    assert!(waiting.is_empty());
}

/// A turn takes its bound of an inbox holding one more, and the next turn takes the one left.
#[test]
fn test_the_message_past_the_bound_waits_for_the_next_turn() {
    let mut waiting = inbox(MESSAGES_MAX + 1);
    let expected: Vec<usize> = waiting.iter().copied().collect();
    let (first, left) = expected.split_at(MESSAGES_MAX);
    assert_eq!(one_turn(&mut waiting), first);
    assert_eq!(one_turn(&mut waiting), left);
}

/// A turn that has taken its bound asks the inbox no more.
#[test]
fn test_a_spent_turn_asks_the_inbox_no_more() {
    let mut turn = InboxTurn::new(1);
    assert_eq!(turn.receive(|| Ok::<_, ()>(1)), Some(1));
    let mut asked = false;
    let next = turn.receive(|| {
        asked = true;
        Ok::<_, ()>(2)
    });
    assert_eq!(next, None);
    assert!(!asked);
}

/// An empty inbox ends the turn's taking and costs it nothing of its bound.
#[test]
fn test_an_empty_inbox_costs_the_turn_nothing() {
    let mut turn = InboxTurn::new(1);
    assert_eq!(turn.receive(|| Err::<usize, _>("empty")), None);
    assert_eq!(turn.receive(|| Ok::<_, ()>(1)), Some(1));
}
