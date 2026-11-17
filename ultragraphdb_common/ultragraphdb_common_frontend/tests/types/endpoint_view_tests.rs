/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An endpoint core's view: corner cases C5.1 to C5.13 and C5.18 of `notes/corner-cases.md` (`PROTOCOL.md` §4.8, §11.12,
//! *Epochs*).

use std::time::{Duration, Instant};

use compio::runtime::Runtime;

use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_frontend::{EndpointView, HoldOutcome};
use ultragraphdb_core_catalog::{CatalogContents, describe_of};
use ultragraphdb_protocol_wire::ErrorReply;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const T1: u64 = 1;

/// A refusal as a test's error.
trait Replied<T> {
    fn replied(self) -> TestResult<T>;
}

impl<T> Replied<T> for Result<T, ErrorReply> {
    fn replied(self) -> TestResult<T> {
        self.map_err(|refusal| TestError::new(format!("{refusal:?}")))
    }
}

/// A point in time from the runtime's timer, whose first tick is now: what the tests count from.
fn start() -> TestResult<Instant> {
    Ok(Runtime::new()?
        .block_on(async { compio::time::interval(Duration::from_secs(1)).tick().await }))
}

/// The bytes of `tenant`'s version `version` holding no declaration, as the metadata group stores them.
fn empty_version(tenant: u64, version: u64) -> TestResult<Vec<u8>> {
    Ok(CatalogContents::new(tenant, version, Vec::new()).encode()?)
}

fn map() -> TestResult<PlacementMap> {
    Ok(PlacementMap::even(2, NodeId::new(1))?)
}

/// A view at the close of `epoch`, holding the placement of tenants 0 and `T1`.
fn at(epoch: u64) -> TestResult<EndpointView> {
    let mut view = EndpointView::new();
    assert!(view.close(epoch, 0, &[]));
    view.placement(map()?, 1, vec![0, T1]);
    Ok(view)
}

/// C5.1, C5.3: a view of no close answers nothing, and wants its placement; its first close and Placement make it answer.
#[test]
fn test_a_view_answers_once_it_holds_a_close_and_a_placement() -> TestResult {
    let mut view = EndpointView::new();
    assert!(!view.answers());
    assert!(view.wants_placement());
    assert_eq!(view.latest_closed(), 0);
    assert!(view.close(1, 0, &[]));
    assert!(!view.answers(), "no placement yet");
    view.placement(map()?, 1, vec![0]);
    assert!(view.answers());
    assert_eq!((view.latest_closed(), view.open_epoch()), (1, 2));
    assert!(view.holds_tenant(0));
    assert!(!view.holds_tenant(T1));
    assert!(view.map().is_some());
    Ok(())
}

/// C5.2: a core that subscribes late begins at the latest close, epoch 40.
#[test]
fn test_a_late_core_begins_at_the_latest_close() -> TestResult {
    let view = at(40)?;
    assert_eq!(view.latest_closed(), 40);
    assert!(view.answers());
    Ok(())
}

/// C5.4: a Begin naming no token, or a token of the closed or the open epoch, is answered at once.
#[test]
fn test_a_token_within_the_open_epoch_is_answered_at_once() -> TestResult {
    let mut view = at(7)?;
    let deadline = start()? + Duration::from_secs(1);
    assert_eq!(view.hold(1, 7, deadline), Some(HoldOutcome::Answered));
    assert_eq!(view.hold(2, 8, deadline), Some(HoldOutcome::Answered));
    assert_eq!(view.next_deadline(), None);
    Ok(())
}

/// C5.5, C5.6: a token of epoch 9 is held until the close of 8; one of 10 until the close of 9.
#[test]
fn test_a_token_ahead_is_held_until_the_close_before_it() -> TestResult {
    let mut view = at(7)?;
    let now = start()?;
    let deadline = now + Duration::from_secs(1);
    assert_eq!(view.hold(9, 9, deadline), None);
    assert_eq!(view.hold(10, 10, deadline), None);
    assert_eq!(view.next_deadline(), Some(deadline));
    assert!(view.close(8, 0, &[]));
    assert_eq!(view.released(now), vec![(9, HoldOutcome::Answered)]);
    assert!(view.close(9, 0, &[]));
    assert_eq!(view.released(now), vec![(10, HoldOutcome::Answered)]);
    assert!(view.released(now).is_empty());
    Ok(())
}

/// C5.7: the deadline passes with no close: timed out, once, and only at the deadline.
#[test]
fn test_a_token_past_its_deadline_times_out() -> TestResult {
    let mut view = at(7)?;
    let deadline = start()? + Duration::from_secs(1);
    assert_eq!(view.hold(1, 9, deadline), None);
    assert!(view.released(deadline - Duration::from_nanos(1)).is_empty());
    assert_eq!(view.released(deadline), vec![(1, HoldOutcome::TimedOut)]);
    assert!(view.released(deadline).is_empty());
    Ok(())
}

/// C5.8: a close arriving one tick before the deadline, or in the tick of the deadline, is taken first.
#[test]
fn test_a_close_arrived_at_the_deadline_is_taken_first() -> TestResult {
    let deadline = start()? + Duration::from_secs(1);
    let mut view = at(7)?;
    assert_eq!(view.hold(1, 9, deadline), None);
    assert!(view.close(8, 0, &[]));
    assert_eq!(view.released(deadline), vec![(1, HoldOutcome::Answered)]);
    Ok(())
}

/// C5.10: a subscription opened again whose first close is the one held applies nothing twice, and reads its placement
/// again.
#[test]
fn test_a_reopened_subscription_at_the_close_held_applies_nothing_twice() -> TestResult {
    let mut view = at(7)?;
    view.reopened();
    assert!(
        !view.answers(),
        "nothing answered before the reopened stream's first close"
    );
    let applied = view.close(7, 0, &[(T1, 9)]);
    assert!(!applied);
    assert_eq!(view.latest_closed(), 7);
    assert_eq!(view.current_version(T1), 0, "nothing applied twice");
    assert!(view.wants_placement());
    assert!(!view.answers());
    Ok(())
}

/// C5.11: a subscription opened again whose first close is 12 wants Placement and the current version of each tenant held
/// read at the open epoch; once read, the view holds what the skipped closes announced.
#[test]
fn test_a_reopened_subscription_reads_what_the_skipped_closes_announced() -> TestResult {
    let mut view = at(7)?;
    assert!(view.close(8, 0, &[(T1, 3)]));
    view.reopened();
    assert!(view.close(12, 0, &[]));
    assert!(!view.answers());
    assert!(view.wants_placement());
    assert_eq!(view.wants_catalogs(), vec![T1]);
    view.placement(map()?, 2, vec![0, T1]);
    view.catalog(T1, 4, 0, &empty_version(T1, 4)?).replied()?;
    assert!(view.answers());
    assert_eq!(view.latest_closed(), 12);
    assert_eq!(view.current_version(T1), 4);
    Ok(())
}

/// C5.12: the version a close names and the core's own Declare raise a tenant's current version, never lower it.
#[test]
fn test_a_tenants_version_never_falls() -> TestResult {
    let mut view = at(7)?;
    assert!(view.close(8, 0, &[(T1, 3)]));
    assert_eq!(view.current_version(T1), 3);
    view.declared(T1, 4, 0);
    assert_eq!(view.current_version(T1), 4);
    assert!(view.close(9, 0, &[(T1, 3)]));
    assert_eq!(view.current_version(T1), 4);
    Ok(())
}

/// The version in effect at the open epoch (`PROTOCOL.md` §6.5): a close of epoch `e` announces versions taking effect at
/// `e + 1`, the open epoch is one above the latest close, and a version committed as pending is in effect nowhere. A
/// tenant never read names none; one holding no version names 0.
#[test]
fn test_the_version_in_effect_at_the_open_epoch() -> TestResult {
    let mut view = at(7)?;
    assert_eq!(view.in_effect(T1), None, "never read");
    view.catalog_none(T1);
    assert_eq!(view.in_effect(T1), Some(0), "the tenant holds none");
    // Close 8 announces version 1, taking effect at 9, the open epoch.
    assert!(view.close(8, 0, &[(T1, 1)]));
    assert_eq!(view.in_effect(T1), Some(1));
    // A Drop's Declare commits version 2 as pending: version 1 stays in effect.
    view.declared(T1, 2, 0);
    assert_eq!(view.current_version(T1), 2);
    assert_eq!(
        view.in_effect(T1),
        Some(1),
        "a pending version is in effect nowhere"
    );
    // Version 3 read with Catalog, taking effect at 11, above the open epoch 9.
    view.catalog(T1, 3, 11, &empty_version(T1, 3)?).replied()?;
    assert_eq!(view.in_effect(T1), Some(1), "version 3 takes effect later");
    assert!(view.close(9, 0, &[]));
    assert_eq!(view.in_effect(T1), Some(1), "the open epoch is 10");
    assert!(view.close(10, 0, &[]));
    assert_eq!(view.in_effect(T1), Some(3), "the open epoch is 11");
    Ok(())
}

/// A declaring Commit's reply names its version's effective epoch, which may lie above the core's open epoch while the
/// close assigning it has not reached the core: the version before stays in effect until that close.
#[test]
fn test_a_declared_version_takes_effect_at_its_epoch() -> TestResult {
    let mut view = at(7)?;
    view.catalog_none(T1);
    view.declared(T1, 1, 9);
    assert_eq!(
        view.in_effect(T1),
        Some(0),
        "version 1 takes effect at 9, the open epoch is 8"
    );
    assert!(view.close(8, 0, &[(T1, 1)]));
    assert_eq!(view.in_effect(T1), Some(1));
    Ok(())
}

/// A view that cannot name the version in effect names none: the current version read is pending, or takes effect above
/// the open epoch, and the version below it was never learned.
#[test]
fn test_a_version_never_learned_leaves_none_named() -> TestResult {
    let mut pending = at(7)?;
    pending
        .catalog(T1, 5, 0, &empty_version(T1, 5)?)
        .replied()?;
    assert_eq!(pending.in_effect(T1), None, "version 4 was never learned");
    let mut later = at(7)?;
    later.catalog(T1, 5, 12, &empty_version(T1, 5)?).replied()?;
    assert_eq!(later.in_effect(T1), None);
    let mut read = at(7)?;
    read.catalog(T1, 5, 3, &empty_version(T1, 5)?).replied()?;
    assert_eq!(read.in_effect(T1), Some(5), "in effect since epoch 3");
    Ok(())
}

/// A close below the latest held is not applied.
#[test]
fn test_an_older_close_lowers_nothing() -> TestResult {
    let mut view = at(7)?;
    assert!(!view.close(5, 0, &[(T1, 2)]));
    assert_eq!(view.latest_closed(), 7);
    assert_eq!(view.current_version(T1), 0);
    Ok(())
}

/// C5.13: a tenant holding no version has the initial catalog's schema id, `describe_of` of no declaration; a version read
/// has the schema id `describe_of` gives its declarations (law L5.4).
#[test]
fn test_a_schema_id_is_describe_ofs() -> TestResult {
    let mut view = at(7)?;
    let initial = describe_of(T1, 0, &[], std::iter::empty()).schema_id();
    assert!(view.unread(T1));
    assert_eq!(view.schema_id(T1), None, "never read");
    view.catalog_none(T1);
    assert!(!view.unread(T1));
    assert_eq!(view.schema_id(T1), Some(initial));
    view.declared(T1, 2, 0);
    assert_eq!(view.schema_id(T1), None, "version 2 is not read");
    view.catalog(T1, 2, 0, &empty_version(T1, 2)?).replied()?;
    assert_eq!(
        view.schema_id(T1),
        Some(initial),
        "no declaration, the same id"
    );
    Ok(())
}

/// Bytes that are no catalog version are refused with code 17, and nothing changes.
#[test]
fn test_bytes_that_do_not_decode_are_refused() -> TestResult {
    let mut view = at(7)?;
    let refused = view
        .catalog(T1, 2, 0, &[0xFF; 3])
        .err()
        .context("a refusal")?;
    assert_eq!(refused.code().number(), 17);
    assert_eq!(view.current_version(T1), 0);
    Ok(())
}

/// C5.18: a close assigning a placement version wants Placement read again.
#[test]
fn test_a_close_assigning_a_placement_wants_it_read() -> TestResult {
    let mut view = at(7)?;
    assert!(view.close(8, 2, &[]));
    assert!(view.wants_placement());
    assert!(!view.answers());
    view.placement(map()?, 2, vec![0, T1]);
    assert!(view.answers());
    assert!(view.close(9, 2, &[]));
    assert!(
        !view.wants_placement(),
        "the version held is not read again"
    );
    Ok(())
}
