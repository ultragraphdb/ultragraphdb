/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_invariant::InvariantError;

#[test]
fn test_the_text_names_the_invariant_violated() {
    assert_eq!(
        InvariantError::Violated("rounds are appended in position order").to_string(),
        "invariant violated: rounds are appended in position order"
    );
    assert_eq!(
        InvariantError::Violated("").to_string(),
        "invariant violated: "
    );
}
