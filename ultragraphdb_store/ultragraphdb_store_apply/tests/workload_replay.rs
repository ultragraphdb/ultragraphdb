/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A seed of the changeset workload replays its run: the same outcome, writes, checks and skipped checks.

use ultragraphdb_store_sim::Seed;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::ChangesetWorkload;

#[test]
fn test_a_seed_replays_its_run() -> TestResult {
    let run = || -> TestResult<(String, usize, usize, usize)> {
        let (outcome, w) = ChangesetWorkload::run(Seed::new(7), ChangesetWorkload::faults(), 200)
            .context("run")?;
        Ok((format!("{outcome:?}"), w.writes(), w.checked(), w.skipped()))
    };
    assert_eq!(run()?, run()?);
    Ok(())
}
