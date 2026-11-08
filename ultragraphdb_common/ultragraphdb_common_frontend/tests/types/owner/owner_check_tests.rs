/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::FutureExt;
use ultragraphdb_cluster_metadata::{MetadataConfig, MetadataState};
use ultragraphdb_common_frontend::{OwnerHandle, ReportAsked};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_protocol_wire::{Declared, DeclaredPredicate, ErrorCode};
use ultragraphdb_store_sim::LogCapture;
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use super::{TestClock, ended, heard, recovered, refused, running, starting, ticking};

const P: &str = "http://ex.org/p";
const Q: &str = "http://ex.org/q";
const AT_FAULT: &str = "<http://ex.org/a>";

/// How many millisecond polls a wait takes before it fails.
const POLLS_MAX: u32 = 5_000;

fn plain(iri: &str) -> Declared {
    Declared::Predicate(DeclaredPredicate::new(iri))
}

fn single(iri: &str) -> Declared {
    Declared::Predicate(DeclaredPredicate::new(iri).with_single(true))
}

/// The version of tenant 0 above `after`, once its epoch is assigned: its number.
async fn assigned(handle: &OwnerHandle, after: u64) -> TestResult<u64> {
    for _ in 0..POLLS_MAX {
        if let Ok(held) = handle.catalog(0, 0).await
            && held.version() > after
            && held.epoch() != 0
        {
            return Ok(held.version());
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!(
        "no version above {after} was assigned its epoch"
    )))
}

/// Partition 1's report that its data refuses `version` of tenant 0, `p` not holding of the term at fault.
fn refusing(version: u64) -> ReportAsked {
    ReportAsked::new(
        Tenant::new(0),
        version,
        1,
        Some((P.to_string(), AT_FAULT.to_string())),
    )
}

/// `p` and `q` declared on tenant 0 and in effect: the version holding them.
async fn both_declared(handle: &OwnerHandle) -> TestResult<u64> {
    let _first = heard(handle.declare(0, 0, vec![plain(P), plain(Q)]).await)?;
    assigned(handle, 0).await
}

/// Whether `capture` holds an event saying `words`.
fn logged(capture: &LogCapture, words: &str) -> bool {
    capture
        .events()
        .iter()
        .any(|event| event.to_string().contains(words))
}

/// A tightening a partition's data refuses is withdrawn by a version restoring what it changed, and its Commit is refused with
/// code 7 naming the predicate and the term at fault; a declaration it left as it was stays.
#[test]
fn test_a_refused_tightening_is_withdrawn_and_its_commit_told() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(8);
    let capture = LogCapture::new();
    capture.capture(|| -> TestResult {
        Runtime::new()?.block_on(async {
            let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
            let (handle, run) = running(owner, &clock);
            let _ticking = ticking(&clock);
            let held = both_declared(&handle).await?;
            let (answer, reported) =
                futures::join!(handle.declare(0, held, vec![single(P)]), async {
                    let tightening = assigned(&handle, held).await?;
                    heard(handle.report(refusing(tightening)).await)
                });
            reported?;
            let refusal = refused(answer)?;
            assert_eq!(refusal.code(), ErrorCode::Refused);
            assert!(refusal.message().contains(P), "{}", refusal.message());
            assert!(
                refusal.message().contains(AT_FAULT),
                "{}",
                refusal.message()
            );
            let restoring = heard(handle.catalog(0, 0).await)?;
            assert_eq!(
                restoring.version(),
                held + 2,
                "the tightening, then the version restoring it"
            );
            drop(handle);
            ended(run).await??;
            Ok(())
        })
    })?;
    assert!(logged(
        &capture,
        "a partition reported on a catalog version"
    ));
    assert!(logged(&capture, "a tightening is withdrawn"));
    Ok(())
}

/// A restarted owner withdraws a tightening its log holds being checked, though no Commit waits on it any more.
#[test]
fn test_a_restarted_owner_withdraws_a_tightening_no_commit_waits_on() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(9);
    Runtime::new()?.block_on(async {
        let start = starting(MetadataConfig::new())?;
        let owner = recovered(&storage, &start, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let ticker = ticking(&clock);
        let held = both_declared(&handle).await?;
        // The Commit is sent and its wait given up at once, as a process stopped after it would.
        let sent = handle.declare(0, held, vec![single(P)]).now_or_never();
        assert!(sent.is_none(), "the tightening waits on its check");
        let tightening = assigned(&handle, held).await?;
        drop(ticker);
        drop(handle);
        ended(run).await??;
        let restarted = recovered(&storage, &start, &clock).await?;
        let (again, rerun) = running(restarted, &clock);
        let _ticking = ticking(&clock);
        heard(again.report(refusing(tightening)).await)?;
        let restoring = assigned(&again, tightening).await?;
        assert_eq!(restoring, tightening + 1);
        drop(again);
        ended(rerun).await??;
        Ok(())
    })
}

/// A withdrawal the group refuses while its pending versions are full waits, says so, and is offered again once a close has
/// announced them; the Commit is then refused.
#[test]
fn test_a_restoring_version_waits_for_room_among_the_pending() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(10);
    let capture = LogCapture::new();
    capture.capture(|| -> TestResult {
        Runtime::new()?.block_on(async {
            let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
            let (handle, run) = running(owner, &clock);
            let ticker = ticking(&clock);
            let held = both_declared(&handle).await?;
            let (answer, filled) =
                futures::join!(handle.declare(0, held, vec![single(P)]), async {
                    let tightening = assigned(&handle, held).await?;
                    drop(ticker);
                    // The report and the offers filling the pending versions are sent in one poll, so one turn takes them all.
                    let asking = handle.clone();
                    let offers = spawn(async move {
                        let tenants = 1..=MetadataState::PENDING_MAX as u64;
                        futures::join!(
                            asking.report(refusing(tightening)),
                            futures::future::join_all(tenants.map(|tenant| asking.declare(
                                tenant,
                                0,
                                vec![plain(P)]
                            )))
                        )
                    });
                    let mut waited = false;
                    for _ in 0..POLLS_MAX {
                        waited = logged(&capture, "a restoring version waits");
                        if waited {
                            break;
                        }
                        compio::time::sleep(Duration::from_millis(1)).await;
                    }
                    assert!(waited, "the restoring version waits");
                    let resumed = ticking(&clock);
                    let (reported, offered) = offers
                        .await
                        .map_err(|error| TestError::new(format!("the offers: {error:?}")))?;
                    heard(reported)?;
                    assert!(offered.iter().all(Result::is_ok), "every offer stands");
                    Ok::<_, TestError>(resumed)
                });
            let _ticker = filled?;
            assert_eq!(refused(answer)?.code(), ErrorCode::Refused);
            drop(handle);
            ended(run).await??;
            Ok(())
        })
    })
}
