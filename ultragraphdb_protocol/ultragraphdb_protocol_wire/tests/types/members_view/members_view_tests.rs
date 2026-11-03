/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyView, Member, MembersView, RoleKind};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::members::members_tests::{golden_members, golden_members_reply};
use crate::utils::golden_bodies::{
    body_data, frame_of, put, section, view_any, with_command_header, with_section,
};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the MembersView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    MembersView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

/// The golden reply with member `index`'s 12 bytes changed by `change`, refused.
fn member_refusal(index: usize, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    let frame = with_section(&bytes("members_reply")?, 3, 0, |s| {
        change(&mut s[12 * index..12 * index + 12]);
    });
    refusal(&frame)
}

#[test]
fn test_the_golden_members_read_their_values() -> TestResult {
    let reply = bytes("members_reply")?;
    let view = MembersView::from_frame(&frame_of(&reply)?)?;
    assert_eq!((view.len(), view.is_empty()), (5, false));
    assert_eq!(view.member(0), Some(Member::new(RoleKind::Compute, 0, 0)));
    assert_eq!(view.member(1), Some(Member::new(RoleKind::Compute, 0, 1)));
    assert_eq!(view.member(2), Some(Member::new(RoleKind::Storage, 2, 2)));
    assert_eq!(view.member(3), Some(Member::new(RoleKind::Storage, 5, 2)));
    assert_eq!(view.member(4), Some(Member::new(RoleKind::Storage, 9, 3)));
    assert_eq!((view.member(5), view.address(5)), (None, None));
    assert_eq!(view.address(0), Some("tcp://compute-a:7400"));
    assert_eq!(view.address(3), Some("tcp://storage-1:7300"));
    assert_eq!(view.address(4), Some("tcp://storage-2:7300"));
    assert_eq!(view.addresses().len(), 4);
    assert_eq!(view.to_body()?, golden_members_reply());
    let request = bytes("members")?;
    let empty = MembersView::from_frame(&frame_of(&request)?)?;
    assert_eq!((empty.len(), empty.is_empty()), (0, true));
    assert_eq!(empty.to_body()?, golden_members());
    assert_eq!(view_any(&reply)?, Ok(()));
    assert_eq!(view_any(&request)?, Ok(()));
    Ok(())
}

#[test]
fn test_the_command_header_is_zero() -> TestResult {
    for at in [0, 127] {
        assert_eq!(
            header_refusal("members_reply", |h| put(h, at, 1, 1))?,
            ProtocolError::reserved_not_zero(128 + at)
        );
    }
    Ok(())
}

#[test]
fn test_the_string_list_then_the_members_length() -> TestResult {
    let reply = bytes("members_reply")?;
    let ragged = with_section(&reply, 3, 0, |s| s.push(0));
    assert_eq!(
        refusal(&ragged)?,
        ProtocolError::section_length(513, 0, 61, 12)
    );
    let both = with_section(&ragged, 3, 1, |s| s.push(0));
    assert_eq!(
        refusal(&both)?,
        ProtocolError::section_length(513, 1, 17, 4)
    );
    Ok(())
}

#[test]
fn test_each_member_is_checked_in_order() -> TestResult {
    for kind in [0, 1, 4, 255] {
        assert_eq!(
            member_refusal(1, |m| m[0] = kind)?,
            ProtocolError::field_value(513, "kind", kind.into())
        );
    }
    // A bad kind is found before padding.
    assert_eq!(
        member_refusal(1, |m| {
            m[0] = 7;
            m[3] = 1;
        })?,
        ProtocolError::field_value(513, "kind", 7)
    );
    let start = section(body_data(&bytes("members_reply")?), 0, 0).0;
    for byte in 1..4 {
        assert_eq!(
            member_refusal(2, |m| m[byte] = 1)?,
            ProtocolError::padding_not_zero(513, start + 24 + byte)
        );
    }
    assert_eq!(
        member_refusal(0, |m| m[4] = 1)?,
        ProtocolError::field_value(513, "partition", 1)
    );
    assert_eq!(
        member_refusal(2, |m| m[4] = 0)?,
        ProtocolError::field_value(513, "partition", 0)
    );
    assert_eq!(
        member_refusal(4, |m| m[8] = 4)?,
        ProtocolError::index_out_of_range(513, 0, 4, 4, 4)
    );
    Ok(())
}

#[test]
fn test_members_come_compute_by_address_then_storage_by_partition() -> TestResult {
    // A compute member after a storage one.
    assert_eq!(
        member_refusal(4, |m| {
            m[0] = 2;
            m[4] = 0;
        })?,
        ProtocolError::field_value(513, "member order", 4)
    );
    // Compute addresses repeated, and descending by their bytes.
    assert_eq!(
        member_refusal(1, |m| m[8] = 0)?,
        ProtocolError::field_value(513, "member order", 1)
    );
    let descending = with_section(&bytes("members_reply")?, 3, 0, |s| {
        s[8] = 1;
        s[20] = 0;
    });
    assert_eq!(
        refusal(&descending)?,
        ProtocolError::field_value(513, "member order", 1)
    );
    // Storage partitions repeated, and descending.
    assert_eq!(
        member_refusal(3, |m| m[4] = 2)?,
        ProtocolError::field_value(513, "member order", 3)
    );
    assert_eq!(
        member_refusal(4, |m| m[4] = 3)?,
        ProtocolError::field_value(513, "member order", 4)
    );
    // Storage members alone, and compute members alone, are in order.
    let storage_only = with_section(&bytes("members_reply")?, 3, 0, |s| {
        *s = s.split_off(24);
    });
    assert_eq!(MembersView::from_frame(&frame_of(&storage_only)?)?.len(), 3);
    let compute_only = with_section(&bytes("members_reply")?, 3, 0, |s| s.truncate(24));
    assert_eq!(MembersView::from_frame(&frame_of(&compute_only)?)?.len(), 2);
    Ok(())
}
