/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Replay checked differentially against the stage-5 apply path on the changeset workload (write-ahead-log, "A round's
//! record is logical"): each round the workload writes is replayed into a replica from the logical writes its
//! execution leaves, and after every step the replica holds every key and value the store holds, versions included.

use ultragraphdb_store_sim::Seed;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::ChangesetWorkload;

#[test]
fn test_replay_derives_every_key_the_workloads_first_apply_wrote() -> TestResult {
    let (mut replayed, mut checked) = (0, 0);
    for seed in 1..=3 {
        let (_, w) = ChangesetWorkload::run_replayed(Seed::new(seed), 200)
            .map_err(|v| v.to_string())
            .context("replay agrees with the first apply")?;
        assert_eq!(
            w.replayed(),
            w.writes(),
            "seed {seed}: every write replayed"
        );
        // Without faults no check is skipped for an injected failure: every step is compared.
        assert_eq!(w.checked(), 200, "seed {seed}: every step compared");
        println!(
            "seed {seed}: {} rounds replayed, {} refused, {} steps compared",
            w.replayed(),
            w.refusals(),
            w.checked()
        );
        replayed += w.replayed();
        checked += w.checked();
    }
    println!("{replayed} rounds replayed, {checked} steps compared");
    assert!(replayed > 100, "only {replayed} rounds replayed");
    Ok(())
}
