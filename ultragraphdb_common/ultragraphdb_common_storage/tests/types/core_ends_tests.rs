/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot;
use futures::executor::block_on;
use futures::poll;
use ultragraphdb_common_storage::CoreEnds;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

type Report = oneshot::Sender<Result<(), RoleError>>;

/// A [`CoreEnds`] waiting on `threads` threads, and what each reports through.
fn waiting_on(threads: usize) -> (CoreEnds, Vec<Report>) {
    let mut ends = CoreEnds::default();
    let reports = (0..threads)
        .map(|_| {
            let (report, ended) = oneshot::channel();
            ends.add(ended);
            report
        })
        .collect();
    (ends, reports)
}

fn report(to: Report, outcome: Result<(), RoleError>) -> TestResult {
    to.send(outcome)
        .map_err(|_| TestError::new("the receiver went away"))
}

/// A clean end a look has heard is not asked for again: a stop racing a core's end exits 0, not 70.
#[test]
fn test_a_clean_end_seen_by_a_look_finishes_clean() -> TestResult {
    let (mut ends, mut reports) = waiting_on(2);
    assert!(!ends.look()?, "nothing has ended");
    report(reports.pop().context("a report")?, Ok(()))?;
    assert!(ends.look()?, "one ended cleanly");
    assert!(!ends.look()?, "it was heard once");
    report(reports.pop().context("a report")?, Ok(()))?;
    block_on(ends.finish())?;
    Ok(())
}

#[test]
fn test_a_failure_is_what_a_look_returns_once() -> TestResult {
    let (mut ends, mut reports) = waiting_on(1);
    let failure = RoleError::io("a store");
    report(reports.pop().context("a report")?, Err(failure.clone()))?;
    assert_eq!(ends.look().err(), Some(failure));
    assert!(!ends.look()?, "nothing is left to hear");
    Ok(())
}

#[test]
fn test_a_thread_gone_without_a_word_is_a_bug() -> TestResult {
    let (mut ends, reports) = waiting_on(1);
    drop(reports);
    let looked = ends.look().err().context("a refusal")?;
    assert_eq!(looked.exit_code(), RoleError::EX_SOFTWARE);
    Ok(())
}

#[test]
fn test_a_thread_gone_without_a_word_finishes_as_a_bug() -> TestResult {
    let (ends, reports) = waiting_on(1);
    drop(reports);
    let finished = block_on(ends.finish()).err().context("a refusal")?;
    assert_eq!(finished.exit_code(), RoleError::EX_SOFTWARE);
    Ok(())
}

/// Every thread is waited on past a failure, since each is flushing, and the first failure is what is reported.
#[test]
fn test_finish_waits_for_every_thread_and_reports_the_first_failure() -> TestResult {
    let (ends, mut reports) = waiting_on(3);
    let (last, middle, first) = (
        reports.pop().context("a report")?,
        reports.pop().context("a report")?,
        reports.pop().context("a report")?,
    );
    let failure = RoleError::io("a store");
    report(first, Err(failure.clone()))?;
    report(middle, Err(RoleError::software("later")))?;
    block_on(async {
        let mut finishing = std::pin::pin!(ends.finish());
        assert!(poll!(&mut finishing).is_pending(), "one is still flushing");
        report(last, Ok(()))?;
        assert_eq!(finishing.await.err(), Some(failure));
        Ok(())
    })
}
