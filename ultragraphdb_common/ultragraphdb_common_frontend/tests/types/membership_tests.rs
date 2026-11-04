/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_frontend::Membership;
use ultragraphdb_protocol_wire::{ErrorCode, Join, RoleKind};
use ultragraphdb_tool_bdd::{Context, TestResult};

const A: &str = "tcp://a:1";
const B: &str = "tcp://b:1";

/// `join` taken by `membership`; the refusal's code and message, if any.
fn joined(membership: &mut Membership, join: &Join) -> TestResult<Option<(ErrorCode, String)>> {
    let taken = membership.join(join);
    Ok(taken
        .err()
        .map(|error| (error.code(), error.message().to_string())))
}

/// Each member's kind, partition and address, in order.
fn listed(membership: &Membership) -> Vec<(RoleKind, u32, String)> {
    let members = membership.members();
    (0..members.len())
        .filter_map(|index| members.get(index))
        .map(|(kind, partition, address)| (kind, partition, address.to_string()))
        .collect()
}

/// Members come compute first by address, then storage by partition.
#[test]
fn test_members_come_in_the_protocols_order() -> TestResult {
    let mut membership = Membership::new(4);
    assert!(joined(&mut membership, &Join::storage(B, vec![3, 4]))?.is_none());
    assert!(joined(&mut membership, &Join::compute("tcp://z:1"))?.is_none());
    assert!(joined(&mut membership, &Join::storage(A, vec![1]))?.is_none());
    assert!(joined(&mut membership, &Join::compute("tcp://c:1"))?.is_none());
    let order: Vec<(RoleKind, u32)> = listed(&membership)
        .iter()
        .map(|(kind, partition, _)| (*kind, *partition))
        .collect();
    assert_eq!(
        order,
        [
            (RoleKind::Compute, 0),
            (RoleKind::Compute, 0),
            (RoleKind::Storage, 1),
            (RoleKind::Storage, 3),
            (RoleKind::Storage, 4),
        ]
    );
    Ok(())
}

/// A claim on a partition another address holds names the partition and its holder, and registers nothing of the Join.
#[test]
fn test_a_held_partition_is_refused_naming_its_holder() -> TestResult {
    let mut membership = Membership::new(4);
    assert!(joined(&mut membership, &Join::storage(A, vec![2]))?.is_none());
    let (code, message) =
        joined(&mut membership, &Join::storage(B, vec![1, 2]))?.context("a refusal")?;
    assert_eq!(code, ErrorCode::Protocol);
    assert!(
        message.contains("partition 2") && message.contains(A),
        "{message}"
    );
    assert_eq!(
        listed(&membership).len(),
        1,
        "partition 1 was not registered"
    );
    Ok(())
}

/// A partition past the placement is refused, naming it; the last partition is inside.
#[test]
fn test_a_partition_outside_the_placement_is_refused() -> TestResult {
    let mut membership = Membership::new(4);
    let (code, message) =
        joined(&mut membership, &Join::storage(A, vec![5]))?.context("a refusal")?;
    assert_eq!(code, ErrorCode::Protocol);
    assert!(message.contains("partition 5"), "{message}");
    assert!(joined(&mut membership, &Join::storage(A, vec![4]))?.is_none());
    Ok(())
}

/// A Join from a registered address replaces what it held: a restart claims again, and may claim less.
#[test]
fn test_a_join_from_a_holder_replaces_its_registration() -> TestResult {
    let mut membership = Membership::new(4);
    assert!(joined(&mut membership, &Join::storage(A, vec![1, 2]))?.is_none());
    assert!(joined(&mut membership, &Join::storage(A, vec![2]))?.is_none());
    let partitions: Vec<u32> = listed(&membership).iter().map(|member| member.1).collect();
    assert_eq!(partitions, [2]);
    assert!(joined(&mut membership, &Join::storage(B, vec![1]))?.is_none());
    assert!(joined(&mut membership, &Join::compute("tcp://c:1"))?.is_none());
    assert!(joined(&mut membership, &Join::compute("tcp://c:1"))?.is_none());
    assert_eq!(
        listed(&membership).len(),
        3,
        "a compute address is one member"
    );
    Ok(())
}
