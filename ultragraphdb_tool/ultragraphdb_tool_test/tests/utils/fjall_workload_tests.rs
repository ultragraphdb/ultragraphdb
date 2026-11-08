/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store workload on the simulated store and fjall at once: every record replayed on fjall, with and without crashes
//! between steps. The store crate's suites run it over more seeds and longer.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::run_both;

#[test]
fn test_both_stores_answer_alike_and_every_record_is_replayed() -> TestResult {
    let run = run_both(1, FaultConfig::none(), 60, 2).context("the stores agree")?;
    assert!(run.replayed() > 0, "{run:?}");
    assert_eq!(run.recoveries(), 0, "{run:?}");
    let (reads, found) = run.versionless_reads();
    assert!(found <= reads && reads <= run.replayed(), "{run:?}");
    Ok(())
}

#[test]
fn test_a_crash_between_steps_reopens_fjall() -> TestResult {
    let run = run_both(2, FaultConfig::none().with_crash_between_steps(100), 60, 2)
        .context("the stores agree across crashes")?;
    assert!(run.recoveries() > 0, "{run:?}");
    Ok(())
}
