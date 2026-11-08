/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{BenchErrorEnum, Clock, RunStart};

/// A party may say it is ready more than once, and then take the clock, which says nothing more; the start, waiting for
/// two parties, goes on once the other has gone.
#[test]
fn test_a_party_ready_twice_takes_the_clock() -> TestResult {
    let (start, mut parties) = RunStart::new(2);
    let gone = parties.pop().context("the second party")?;
    let mut party = parties.pop().context("the first party")?;
    party.ready();
    party.ready();
    drop(gone);
    let run = start.start();
    // Read in this order, the party's clock first: the one the start returned, it reads no more than the start's.
    let (taken, run_elapsed) = (party.clock()?.elapsed(), run.elapsed());
    assert!(taken <= run_elapsed, "{taken:?} {run_elapsed:?}");
    Ok(())
}

/// A party whose run's start was dropped unstarted learns it, rather than waiting for a clock that never comes.
#[test]
fn test_a_party_learns_that_the_start_is_gone() -> TestResult {
    let (start, mut parties) = RunStart::new(1);
    let party = parties.pop().context("the party")?;
    drop(start);
    let refused = party.clock();
    assert!(
        matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Commit(_))),
        "{refused:?}"
    );
    Ok(())
}
