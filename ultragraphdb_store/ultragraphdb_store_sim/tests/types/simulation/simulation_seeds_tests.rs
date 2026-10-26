/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fixed seed set: every fault class over the store workload, run in `make test`. Longer runs over fresh seeds
//! are the `store_sim_fuzz` binary's.

use ultragraphdb_store_sim::{FaultConfig, Seed, StoreWorkload};

#[test]
fn test_the_fixed_seeds_break_no_invariant() {
    for seed in 0..48 {
        if let Err(violation) = StoreWorkload::run(Seed::new(seed), FaultConfig::mixed(), 300, 2) {
            panic!("{violation}");
        }
    }
}

#[test]
fn test_the_fixed_seeds_break_no_invariant_without_faults() {
    for seed in 100..116 {
        if let Err(violation) = StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 3) {
            panic!("{violation}");
        }
    }
}

/// The keyspace without versions read through snapshots and through the store, with every fault class and without.
#[test]
fn test_the_fixed_seeds_with_versionless_reads_break_no_invariant() {
    for seed in 200..232 {
        let faults = if seed % 4 == 0 {
            FaultConfig::none()
        } else {
            FaultConfig::mixed()
        };
        if let Err(violation) =
            StoreWorkload::new(2)
                .with_versionless_reads()
                .run_on(Seed::new(seed), faults, 300)
        {
            panic!("{violation}");
        }
    }
}

/// Seeds on which a model with the defect each names breaks an invariant, replayed as `store_sim_fuzz` runs them, with
/// every fault class and three readers, to a few steps past the step it fails at.
#[test]
fn test_the_seeds_the_fuzzer_found_break_no_invariant() {
    let found = [
        // A snapshot opened before a drop reads a corrupted version the drop removed: a model lifting the key's taint
        // for every reader at the drop fails at step 67.
        (302, 70),
        // A power loss keeps or loses a prefix drop that rewrote the schema key, a recovery the contents cannot
        // decide: a model adopting the first outcome they match collects the version the drop fenced at a later
        // compaction, at steps 1202 and 1333.
        (5495, 1210),
        (2900, 1340),
    ];
    for (seed, steps) in found {
        if let Err(violation) = StoreWorkload::run(Seed::new(seed), FaultConfig::mixed(), steps, 3)
        {
            panic!("{violation}");
        }
    }
}

/// Seeds whose readers make a read again through an open snapshot after a heal of a corruption it read, run as
/// `store_sim_fuzz` runs them. A store whose open snapshots read the heal fails each: one whose snapshots read the
/// store's live corruptions fails seed 584 at step 35, a point read a drop healed, and seed 1001 at step 108, a scan;
/// one whose snapshots read a version written again since fails seed 1853 at step 363.
#[test]
fn test_the_seeds_that_reread_through_a_heal_break_no_invariant() {
    for (seed, steps) in [(584, 40), (1001, 110), (1853, 370)] {
        if let Err(violation) = StoreWorkload::run(Seed::new(seed), FaultConfig::mixed(), steps, 3)
        {
            panic!("{violation}");
        }
    }
}
