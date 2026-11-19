/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_common_system::ReportAsked;
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transactions_log::SimStorage;

use super::{
    TestClock, ended, heard, principal, recovered, refused, running, starting, with_principal,
};

/// An ask naming what no value of its kind may be is refused with code 1 naming it, and the owner goes on: a Lease of type
/// prefix 0, a close request and a cursor past the largest epoch, a report of partition 0. A session registered for a principal
/// the group does not hold is refused with code 7.
#[test]
fn test_an_ask_naming_no_value_of_its_kind_is_refused() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(1);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let past = Epoch::MAX.get() + 1;
        let lease = refused(handle.lease(0, 1, 8).await)?;
        assert_eq!(lease.code(), ErrorCode::Protocol);
        assert!(
            lease.message().contains("type prefix 0"),
            "{}",
            lease.message()
        );
        let close = refused(handle.close_request(past).await)?;
        assert_eq!(close.code(), ErrorCode::Protocol);
        assert!(
            close.message().contains(&past.to_string()),
            "{}",
            close.message()
        );
        let cursor = refused(handle.next_close(past).await)?;
        assert_eq!(cursor.code(), ErrorCode::Protocol);
        assert!(
            cursor.message().contains(&past.to_string()),
            "{}",
            cursor.message()
        );
        let report = ReportAsked::new(Tenant::new(0), 1, 0, None);
        assert_eq!(
            refused(handle.report(report).await)?.code(),
            ErrorCode::Protocol
        );
        // Identities are drawn from 1 up, one per principal.
        let unknown = u64::MAX;
        let registered = refused(handle.register_session(1, unknown).await)?;
        assert_eq!(registered.code(), ErrorCode::Refused);
        assert!(
            registered.message().contains(&unknown.to_string()),
            "{}",
            registered.message()
        );
        assert!(heard(handle.lease(1, 1, 8).await)? > 0, "the owner goes on");
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// A Session evicting a session and a Commit's Check of that session, taken in one turn, answer the check 12: the
/// eviction is appended though not yet durable, and the close carrying it could reach a partition before the commit.
#[test]
fn test_a_session_evicted_in_the_same_turn_is_refused_its_commit() -> TestResult {
    use super::system_group_sessions_tests::{HOLD_US, MARKS, T_MS, connected, outcome};
    let clock = TestClock::new();
    let storage = SimStorage::new(2);
    Runtime::new()?.block_on(async {
        let start =
            starting(MetadataConfig::new().with_sessions_max(1)?)?.with_transaction_ms(T_MS);
        let mut owner = recovered(&storage, &start, &clock).await?;
        with_principal(&mut owner, "ann").await?;
        let (handle, run) = running(owner, &clock);
        let ann = principal(&handle, "ann").await?;
        // Past the quiet time, in which a full registry registers nothing.
        clock.advance(HOLD_US);
        heard(handle.session(1, ann).await)?;
        let connection = connected(&handle).await?;
        // Both asks are sent before the owner's run is polled again, so one turn takes them in this order.
        let (second, commit) = futures::join!(
            handle.session(2, ann),
            outcome(&handle, connection, 1, ann, MARKS)
        );
        heard(second)?;
        assert_eq!(commit?, 12);
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}
