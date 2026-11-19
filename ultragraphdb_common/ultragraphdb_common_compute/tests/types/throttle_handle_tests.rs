/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_compute::{AuthThrottle, LoginLimits};
use ultragraphdb_tool_bdd::TestResult;

const PEER: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const OTHER: IpAddr = IpAddr::V4(Ipv4Addr::BROADCAST);
const BAN: Duration = Duration::from_secs(60);

/// A throttle served by message counts what the cores tell it in the order they told it: a source's first throttled attempt
/// waits 100 ms, as `AuthThrottle` documents, the failures past those ban it, a success forgets a source's failures, and a
/// Unix socket's peer waits nothing (design D15).
#[test]
fn test_a_served_throttle_counts_what_it_is_told() -> TestResult {
    Runtime::new()?.block_on(async {
        let (logins, run) = AuthThrottle::new(LoginLimits::new(2, 2, BAN)).serve();
        spawn(run).detach();
        assert_eq!(logins.delay(None).await, Duration::ZERO, "no address");
        for _ in 0..3 {
            logins.outcome(PEER, false);
        }
        assert_eq!(logins.delay(Some(PEER)).await, Duration::from_millis(100));
        assert!(logins.admits(PEER).await, "one failure past the threshold");
        logins.outcome(PEER, false);
        assert!(
            !logins.admits(PEER).await,
            "two failures past the threshold"
        );
        for _ in 0..3 {
            logins.outcome(OTHER, false);
        }
        logins.outcome(OTHER, true);
        assert_eq!(logins.delay(Some(OTHER)).await, Duration::ZERO, "forgotten");
        assert!(logins.admits(OTHER).await);
        Ok(())
    })
}

/// A throttle that stopped admits no connection and delays no attempt, as compute is stopping.
#[test]
fn test_a_stopped_throttle_admits_no_one() -> TestResult {
    Runtime::new()?.block_on(async {
        let (logins, run) = AuthThrottle::new(LoginLimits::default()).serve();
        drop(run);
        assert!(!logins.admits(PEER).await);
        assert_eq!(logins.delay(Some(PEER)).await, Duration::ZERO);
        logins.outcome(PEER, false);
        Ok(())
    })
}
