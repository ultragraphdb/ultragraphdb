/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fixed seed set of the posting workload: a hermetic test of the posting lists on the simulated store, with
//! and without faults.

use ultragraphdb_store_sim::{FaultConfig, Seed};

use crate::utils::posting_workload::PostingWorkload;

#[test]
fn test_seeds_without_faults() {
    let (mut reads, mut rollups) = (0, 0);
    for seed in 1..=20 {
        let (outcome, r, w) = PostingWorkload::run(Seed::new(seed), FaultConfig::none(), 400, 2)
            .unwrap_or_else(|v| panic!("{v}"));
        assert!(!outcome.contents().keys().is_empty());
        reads += r;
        rollups += w;
    }
    assert!(reads > 1000, "only {reads} reads compared");
    assert!(rollups > 50, "only {rollups} rollups written");
}

#[test]
fn test_seeds_with_faults() {
    let mut reads = 0;
    for seed in 100..=139 {
        let (_, r, _) = PostingWorkload::run(Seed::new(seed), PostingWorkload::faults(), 400, 2)
            .unwrap_or_else(|v| panic!("{v}"));
        reads += r;
    }
    assert!(reads > 1000, "only {reads} reads compared");
}

#[test]
fn test_a_seed_replays_its_run_and_its_events() {
    let run = || {
        let capture = ultragraphdb_store_sim::LogCapture::new();
        let outcome = tracing::subscriber::with_default(capture.subscriber(), || {
            PostingWorkload::run(Seed::new(7), PostingWorkload::faults(), 300, 2).expect("run")
        });
        (outcome, capture.events())
    };
    let (first, first_events) = run();
    let (again, again_events) = run();
    assert_eq!(first.0, again.0);
    assert_eq!((first.1, first.2), (again.1, again.2));
    assert_eq!(first_events, again_events);
    assert!(
        !first_events.is_empty(),
        "the faults of the run are reported"
    );
}
