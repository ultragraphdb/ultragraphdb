/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_service::{ACCEPT_PAUSE_FIRST, ACCEPT_PAUSE_MAX, AcceptPace};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The pauses of a run of failures, up to and one past the first that reaches the maximum.
fn pauses_to_the_maximum() -> TestResult<Vec<Duration>> {
    let mut pace = AcceptPace::default();
    let mut pauses = Vec::new();
    // A doubling reaches any `Duration` within its bits.
    for _ in 0..u64::BITS {
        pauses.push(pace.failed());
        if pauses.last() == Some(&ACCEPT_PAUSE_MAX) {
            pauses.push(pace.failed());
            return Ok(pauses);
        }
    }
    None.context("a pause reaching ACCEPT_PAUSE_MAX")
}

/// Each failure pauses twice as long as the one before, from the first pause up to the maximum, and the failure at the maximum
/// and the one past it both pause the maximum.
#[test]
fn test_the_pause_doubles_to_its_maximum_and_stays_there() -> TestResult {
    let pauses = pauses_to_the_maximum()?;
    assert_eq!(pauses.first(), Some(&ACCEPT_PAUSE_FIRST));
    let (past, reaching) = pauses.split_last().context("two pauses")?;
    let (at, growing) = reaching.split_last().context("the pause at the maximum")?;
    for pair in growing.windows(2) {
        let (before, after) = (
            pair.first().context("a pause")?,
            pair.last().context("its next")?,
        );
        assert_eq!(*after, before.saturating_mul(2), "{pauses:?}");
    }
    let below = growing.last().context("a pause below the maximum")?;
    assert!(*below < ACCEPT_PAUSE_MAX, "{pauses:?}");
    assert_eq!(*at, below.saturating_mul(2).min(ACCEPT_PAUSE_MAX));
    assert_eq!(*past, ACCEPT_PAUSE_MAX, "{pauses:?}");
    Ok(())
}

/// An accepted connection ends the run: it says how many failures it ended, and the next failure pauses the first pause again.
#[test]
fn test_an_accepted_connection_ends_the_run_of_failures() -> TestResult {
    let mut pace = AcceptPace::default();
    let run = pauses_to_the_maximum()?.len();
    for _ in 0..run {
        let _pause = pace.failed();
    }
    assert_eq!(pace.failures(), u32::try_from(run)?);
    assert_eq!(pace.accepted(), u32::try_from(run)?);
    assert_eq!(pace.failures(), 0);
    assert_eq!(pace.accepted(), 0);
    assert_eq!(pace.failed(), ACCEPT_PAUSE_FIRST);
    Ok(())
}

/// An accept loop's step tries again after each failure, pausing between, and returns the connection the next try gives with the
/// run ended.
#[test]
fn test_accept_tries_after_each_failure_and_returns_the_connection() -> TestResult {
    let failing = 2;
    let tries = Cell::new(0);
    let mut pace = AcceptPace::default();
    let accepted = Runtime::new()?.block_on(pace.accept("test", || {
        let tried = tries.get() + 1;
        tries.set(tried);
        async move {
            if tried <= failing {
                Err("refused")
            } else {
                Ok(tried)
            }
        }
    }));
    assert_eq!(accepted, failing + 1);
    assert_eq!(tries.get(), failing + 1);
    assert_eq!(pace.failures(), 0);
    Ok(())
}
