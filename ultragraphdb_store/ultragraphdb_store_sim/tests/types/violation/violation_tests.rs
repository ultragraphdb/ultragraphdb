/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_store_sim::{Breach, Invariant, Seed, Violation};

#[test]
fn test_a_violation_names_its_seed_step_and_invariant() {
    let violation = Violation::new(
        Seed::new(5),
        17,
        Breach::new(Invariant::DroppedPrefixAbsent, "key 00"),
    );
    assert_eq!((violation.seed(), violation.step()), (Seed::new(5), 17));
    assert_eq!(
        (violation.invariant(), violation.detail()),
        (Invariant::DroppedPrefixAbsent, "key 00")
    );
    assert_eq!(
        violation.to_string(),
        "seed 5 step 17 broke the invariant that a dropped prefix stays dropped: key 00; replay it with seed 5"
    );
    assert!(violation.source().is_none());
}
