/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_invariant::{InvariantError, InvariantErrorEnum};

#[test]
fn test_the_constructor_names_the_invariant() {
    let error = InvariantError::Violated("rounds are appended in position order");
    assert_eq!(
        error.0,
        InvariantErrorEnum::Violated {
            name: "rounds are appended in position order"
        }
    );
}

#[test]
fn test_violations_are_equal_by_their_invariant() {
    assert_eq!(InvariantError::Violated("a"), InvariantError::Violated("a"));
    assert_ne!(InvariantError::Violated("a"), InvariantError::Violated("b"));
}
