/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{BodyBuilder, Commit, OperationResult};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

/// generate_bodies.py's session: bytes 0x11 to 0x20.
pub fn session() -> [u8; 16] {
    std::array::from_fn(|i| 0x11 + i.to_le_bytes()[0])
}

/// The golden Commit reply: request 7 at commit version (30 << 22) | 100, the token of partition 2's round at
/// position 41 in epoch 30, an insert-if-absent that found <http://ex.org/x>, a counter add of -5, a compare-and-set.
///
/// # Errors
///
/// When epoch 30 is refused.
pub fn golden_commit() -> TestResult<Commit> {
    let mut commit = Commit::new(session(), 7, (30 << 22) | 100).with_token(SessionToken::new(
        2,
        41,
        Epoch::new(30)?,
    ));
    commit.push_node_result("http://ex.org/x", false);
    commit.push_result(OperationResult::counter(-5));
    commit.push_result(OperationResult::compare_and_set());
    Ok(commit)
}

#[test]
fn test_commit_builds_the_golden_reply() -> TestResult {
    let commit = golden_commit()?;
    assert_eq!(commit.session(), session());
    assert_eq!(commit.request(), 7);
    assert_eq!(commit.commit_version(), (30 << 22) | 100);
    assert_eq!(commit.token().map(SessionToken::position), Some(41));
    assert_eq!(
        commit.results(),
        &[
            OperationResult::for_node(0, false),
            OperationResult::counter(-5),
            OperationResult::compare_and_set()
        ][..]
    );
    assert_eq!(commit.node(0), Some("http://ex.org/x"));
    assert_eq!((commit.node(1), commit.node(3)), (None, None));
    assert_eq!(
        commit.frame(golden("commit")?.header())?,
        golden("commit")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_commit_builds_the_golden_request() -> TestResult {
    let commit = Commit::new(session(), 7, 0);
    assert_eq!(commit.token(), None);
    assert!(commit.results().is_empty());
    assert_eq!(
        commit.frame(golden("commit_request")?.header())?,
        golden("commit_request")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_a_node_found_twice_is_interned_once() -> TestResult {
    let mut commit = Commit::new(session(), 8, (31 << 22) | 2).with_token(SessionToken::new(
        1,
        1,
        Epoch::new(31)?,
    ));
    commit.push_node_result("http://ex.org/y", true);
    commit.push_node_result("https://w3id.org/ultragraphdb/ns/tenant/0/.well-known/genid/00002a-0123456789abcdef0123456789", true);
    commit.push_node_result("http://ex.org/y", false);
    assert_eq!(
        commit.results(),
        &[
            OperationResult::for_node(0, true),
            OperationResult::for_node(1, true),
            OperationResult::for_node(0, false)
        ][..]
    );
    assert_eq!(commit.node(2), Some("http://ex.org/y"));
    Ok(())
}
