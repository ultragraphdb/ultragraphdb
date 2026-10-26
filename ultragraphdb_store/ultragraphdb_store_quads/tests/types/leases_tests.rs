/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_store_quads::{LEASE_BLOCK, Leases};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};

#[test]
fn test_a_crash_reissues_no_uid_and_sequences_are_apart() {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut handed = BTreeSet::new();
    for _ in 0..3 {
        let leases = Leases::new();
        for _ in 0..(LEASE_BLOCK + 7) {
            assert!(
                handed.insert(leases.next_uid(&store, 0).unwrap()),
                "a UID is never reissued"
            );
        }
        // A crash loses what the process held; the synced mark survives.
        store.crash();
        store.recover();
    }
    assert!(!handed.contains(&0));
    let leases = Leases::new();
    assert_eq!(
        leases.next_statement_id(&store, 0).unwrap(),
        1,
        "statement IDs are a sequence of their own"
    );
    assert_eq!(
        leases.next_uid(&store, 9).unwrap(),
        1,
        "each namespace has its own"
    );
}
