/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{Breach, Invariant};

#[test]
fn test_a_breach_names_the_invariant_and_how_it_broke() {
    let breach = Breach::new(Invariant::BatchWhole, "2 of 3 entries");
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (Invariant::BatchWhole, "2 of 3 entries")
    );
    assert_eq!(
        breach.to_string(),
        "broke the invariant that a batch is visible whole or not at all: 2 of 3 entries"
    );
}
