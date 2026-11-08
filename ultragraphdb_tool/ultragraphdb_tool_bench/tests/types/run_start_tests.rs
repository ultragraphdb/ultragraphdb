/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{Clock, RunStart, WallClock};

/// How long the slow party's setup takes, as opening a log takes on a loaded machine.
const SETUP: Duration = Duration::from_millis(20);

/// A party whose setup outlasts another's does not shorten the run: the clock starts after the last party is ready, so
/// it reads at least the slow setup less than a clock started when the first party was. A clock started before the
/// setups would read more, and a partition writing for a run's length would lose the setup's time from it.
#[test]
fn test_a_slow_partys_setup_does_not_count_against_the_run() -> TestResult {
    let (start, mut parties) = RunStart::new(2);
    let mut slow = parties.pop().context("the second party")?;
    let mut quick = parties.pop().context("the first party")?;
    quick.ready();
    let first_ready = WallClock::new();
    std::thread::sleep(SETUP);
    slow.ready();
    let run = start.start();
    // Read in this order, the run's clock first: it started at least `SETUP` after `first_ready`.
    let (run_elapsed, first_elapsed) = (run.elapsed(), first_ready.elapsed());
    assert!(
        run_elapsed + SETUP <= first_elapsed,
        "{run_elapsed:?} {first_elapsed:?}"
    );
    // Each party takes the clock the start returned, started no earlier than the slow party's setup ended.
    for party in [quick, slow] {
        let (taken, since_first) = (party.clock()?.elapsed(), first_ready.elapsed());
        assert!(taken + SETUP <= since_first, "{taken:?} {since_first:?}");
    }
    Ok(())
}

/// A party gone before it is ready, as a partition whose log failed to open, does not hold the start of the others.
#[test]
fn test_a_party_gone_does_not_hold_the_start() -> TestResult {
    let (start, mut parties) = RunStart::new(2);
    let mut kept = parties.pop().context("the second party")?;
    drop(parties);
    kept.ready();
    let run = start.start();
    // Read in this order, the party's clock first: the one the start returned, it reads no more than the start's.
    let (taken, run_elapsed) = (kept.clock()?.elapsed(), run.elapsed());
    assert!(taken <= run_elapsed, "{taken:?} {run_elapsed:?}");
    Ok(())
}

/// A run of no parties starts at once.
#[test]
fn test_a_run_of_no_parties_starts_at_once() {
    let (start, parties) = RunStart::new(0);
    assert!(parties.is_empty());
    let run = start.start();
    assert!(run.elapsed() < Duration::from_secs(60));
}
