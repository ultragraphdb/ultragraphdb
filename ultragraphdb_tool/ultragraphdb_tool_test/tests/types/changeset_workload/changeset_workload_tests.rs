/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The changeset workload over a store that keeps every rule: no check reports a violation, and every step is checked.
//! The store crates' suites run it longer and under faults.

use ultragraphdb_store_sim::{FaultConfig, Seed};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::ChangesetWorkload;

/// Steps of a run: enough for writes, reifiers and administration to follow one another.
const STEPS: u64 = 60;

#[test]
fn test_a_store_keeping_every_rule_breaks_none() -> TestResult {
    let (_, w) = ChangesetWorkload::run(Seed::new(1), FaultConfig::none(), STEPS)
        .map_err(|v| v.to_string())
        .context("the workload breaks no rule")?;
    // Without faults every step is checked, and compared with the model, which no recovery has ended.
    assert_eq!(w.checked, usize::try_from(STEPS)?);
    assert_eq!(w.compared, w.checked);
    assert!(w.writes > 0, "{} writes", w.writes);
    Ok(())
}

#[test]
fn test_a_replayed_run_replays_every_write() -> TestResult {
    let (_, w) = ChangesetWorkload::run_replayed(Seed::new(2), STEPS)
        .map_err(|v| v.to_string())
        .context("replay agrees with the first apply")?;
    assert_eq!(w.replayed, w.writes);
    assert_eq!(w.checked, usize::try_from(STEPS)?);
    assert!(w.writes > 0, "{} writes", w.writes);
    Ok(())
}
