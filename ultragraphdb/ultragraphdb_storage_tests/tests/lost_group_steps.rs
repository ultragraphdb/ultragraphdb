/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A storage process that does not host the system group, over the system group the test plays, which breaks the
//! connection the process subscribes on (`PROTOCOL.md` §2.5, step 3; §2.6; §11.12, *Joining*): the process closes it, if the
//! group has not, joins and subscribes again on a new one, and runs on. Or it breaks a catalog version's frames on the connection
//! the process reads its catalog on as it starts (§13.7): a late frame is dropped, and a frame on a stream that answers no
//! request closes the connection, the catalog read again on a new one.

use compio::runtime::Runtime;
use cucumber::{then, when};
use service_utils::WaitStrategy;
use std::time::Duration;
use ultragraphdb_tests::{STORAGE_BINARY, not_hosting, role, scratch, storage_holding, with};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::played_system::{
    Push, STRAY_AHEAD, exit_of, play_system, played_system_address, until_seen,
};
use crate::{Steps, services};

/// What a broken subscription's When saw, for its Then.
#[derive(Debug, Default)]
pub(crate) struct LostSteps {
    /// The subscriptions' connections that had ended when the second Join arrived, held.
    ended_at_rejoin: usize,
    /// The subscriptions opened, once the process subscribed again.
    subscribes: usize,
    /// Whether the process still ran once it subscribed again, or joined.
    ran_on: bool,
    /// The connections a catalog version was asked on, once the process joined.
    catalog_connections: usize,
    /// The catalog reads' connections that had ended when a catalog version was first asked on a second connection.
    catalog_ended_at_second: usize,
}

/// A storage process over the played system group, which breaks its first subscription's connection by `push`: each
/// Join after the first is held until the test has read what had ended by then, and the process is stopped once it
/// subscribed again.
fn broken(w: &mut Steps, push: Push) -> TestResult {
    let log = scratch("storage-broken-subscription.log");
    let settings = with(
        not_hosting(storage_holding()?),
        "cluster.system_address",
        &format!("tcp://{}", played_system_address()?),
    );
    let util = services()?;
    Runtime::new()?.block_on(async {
        let played = play_system(push).await?;
        played.hold_rejoins(true);
        let pid = util.start_service_from_config(role(
            STORAGE_BINARY,
            &settings,
            &log,
            WaitStrategy::NoWait,
        )?)?;
        until_seen(|| Ok(played.joined()?.len() > 1), "a second Join").await?;
        w.lost.ended_at_rejoin = played.subscriptions_ended();
        played.hold_rejoins(false);
        until_seen(|| Ok(played.subscribes() > 1), "a second Subscribe").await?;
        w.lost.subscribes = played.subscribes();
        w.lost.ran_on = util.wait_for_exit(pid, Duration::ZERO).is_err();
        let signalled = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()?;
        if !signalled.success() && w.lost.ran_on {
            return Err(TestError::new("the process could not be stopped"));
        }
        w.exited = exit_of(&util, pid).await;
        w.played = Some(played);
        Ok::<_, TestError>(())
    })?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[when(
    "the system group ends the connection a storage process that does not host it subscribes on with Error 1 on stream 0"
)]
fn connection_error(w: &mut Steps) -> TestResult {
    broken(w, Push::ConnectionError)
}

#[when(
    "the system group sends a storage process that does not host it a frame on a stream it has not opened, on the connection it subscribes on"
)]
fn stray_ahead(w: &mut Steps) -> TestResult {
    broken(w, Push::StrayAhead)
}

/// A storage process over the played system group, which breaks a catalog version's frames by `push` on the first
/// connection the process reads its catalog on: the process is stopped once it joined.
fn catalog_broken(w: &mut Steps, push: Push) -> TestResult {
    let log = scratch("storage-broken-catalog.log");
    let settings = with(
        not_hosting(storage_holding()?),
        "cluster.system_address",
        &format!("tcp://{}", played_system_address()?),
    );
    let util = services()?;
    Runtime::new()?.block_on(async {
        let played = play_system(push).await?;
        let pid = util.start_service_from_config(role(
            STORAGE_BINARY,
            &settings,
            &log,
            WaitStrategy::NoWait,
        )?)?;
        let joined = until_seen(|| Ok(!played.joined()?.is_empty()), "a Join").await;
        w.lost.catalog_connections = played.catalog_connections();
        w.lost.catalog_ended_at_second = played.catalog_ended_at_second();
        w.lost.ran_on = util.wait_for_exit(pid, Duration::ZERO).is_err();
        let signalled = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()?;
        if !signalled.success() && w.lost.ran_on {
            return Err(TestError::new("the process could not be stopped"));
        }
        w.exited = exit_of(&util, pid).await;
        w.played = Some(played);
        joined
    })?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[when(
    "the system group answers a storage process that does not host it a catalog version in two Catalog frames, and sends between them an Error on a stream the process has ended"
)]
fn late_in_catalog(w: &mut Steps) -> TestResult {
    catalog_broken(w, Push::LateInCatalog)
}

#[when(
    "the system group sends a storage process that does not host it the second of a catalog version's Catalog frames on stream 0"
)]
fn catalog_on_stream_zero(w: &mut Steps) -> TestResult {
    catalog_broken(w, Push::StrayInCatalog(0))
}

#[when(
    "the system group sends a storage process that does not host it the second of a catalog version's Catalog frames on a stream it has not opened"
)]
fn catalog_ahead(w: &mut Steps) -> TestResult {
    // Catalog version 1 is the first request on its connection, on stream 1.
    catalog_broken(w, Push::StrayInCatalog(1 + STRAY_AHEAD))
}

/// The process joined and ran on until the test stopped it, then stopped cleanly.
fn joined_and_ran_on(w: &Steps) -> TestResult {
    assert!(w.lost.ran_on, "it exited:\n{}", w.log);
    let exited = w.exited.context("the process's exit")?;
    assert_eq!(exited.code(), Some(0), "{}", w.log);
    Ok(())
}

#[then("the process reads the version whole on that connection, joins, and runs on")]
fn read_whole(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.lost.catalog_connections, 1,
        "the catalog was read on more than one connection:\n{}",
        w.log
    );
    joined_and_ran_on(w)
}

#[then(
    "the process closes that connection before it reads the catalog again on a new one, joins, and runs on"
)]
fn catalog_read_again(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.lost.catalog_connections, 2,
        "the catalog was not read again on one new connection:\n{}",
        w.log
    );
    assert_eq!(
        w.lost.catalog_ended_at_second, 1,
        "the first catalog read's connection was open when the second asked:\n{}",
        w.log
    );
    joined_and_ran_on(w)
}

/// The process subscribed again and ran on until the test stopped it, then stopped cleanly.
fn ran_on(w: &Steps) -> TestResult {
    assert!(w.lost.subscribes > 1, "{}", w.log);
    assert!(w.lost.ran_on, "it exited:\n{}", w.log);
    let exited = w.exited.context("the process's exit")?;
    assert_eq!(exited.code(), Some(0), "{}", w.log);
    Ok(())
}

#[then("the process joins and subscribes again on a new connection, and runs on")]
fn joined_again(w: &mut Steps) -> TestResult {
    ran_on(w)
}

#[then(
    "the process closes that connection before it joins again on a new one, subscribes again, and runs on"
)]
fn closed_then_joined(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.lost.ended_at_rejoin, 1,
        "the first subscription's connection was open at the second Join:\n{}",
        w.log
    );
    ran_on(w)
}
