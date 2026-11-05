/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::WalkEnd;

#[test]
fn test_what_a_walk_gave_ends_just_past_its_last_key() {
    let past = Some(vec![7, 0]);
    let stopped = WalkEnd::Stopped {
        last: vec![7],
        next: vec![9],
    };
    let paused = WalkEnd::Paused {
        last: Some(vec![7]),
        next: vec![7],
    };
    assert_eq!(stopped.past_last(), past);
    assert_eq!(paused.past_last(), past);
}

#[test]
fn test_a_walk_exhausted_or_having_given_nothing_has_nothing_to_be_past() {
    let nothing = WalkEnd::Paused {
        last: None,
        next: vec![7],
    };
    assert_eq!(nothing.past_last(), None);
    assert_eq!(WalkEnd::Exhausted.past_last(), None);
}
