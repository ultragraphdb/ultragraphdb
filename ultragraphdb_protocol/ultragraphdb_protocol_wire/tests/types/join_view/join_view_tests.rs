/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyView, JoinView, RoleKind};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::join::join_tests::{golden_join_compute, golden_join_storage};
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the JoinView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    JoinView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

#[test]
fn test_the_golden_joins_read_their_values() -> TestResult {
    let storage = bytes("join_storage")?;
    let view = JoinView::from_frame(&frame_of(&storage)?)?;
    assert_eq!(view.kind(), RoleKind::Storage);
    assert_eq!(view.address(), "tcp://storage-1:7300");
    assert_eq!(view.partition_count(), 3);
    assert_eq!(
        (0..4).map(|i| view.partition(i)).collect::<Vec<_>>(),
        vec![Some(2), Some(5), Some(9), None]
    );
    assert_eq!(view.to_body()?, golden_join_storage());
    let compute = bytes("join_compute")?;
    let second = JoinView::from_frame(&frame_of(&compute)?)?;
    assert_eq!(second.kind(), RoleKind::Compute);
    assert_eq!(second.address(), "unix:///run/ugdb/compute.sock");
    assert_eq!((second.partition_count(), second.partition(0)), (0, None));
    assert_eq!(second.to_body()?, golden_join_compute());
    assert_eq!(view_any(&storage)?, Ok(()));
    assert_eq!(view_any(&compute)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_bytes_then_the_kind_are_checked() -> TestResult {
    assert_eq!(
        header_refusal("join_storage", |h| put(h, 1, 1, 1))?,
        ProtocolError::reserved_not_zero(129)
    );
    assert_eq!(
        header_refusal("join_storage", |h| {
            put(h, 0, 9, 1);
            put(h, 127, 1, 1);
        })?,
        ProtocolError::reserved_not_zero(255)
    );
    for kind in [0, 1, 4, 255] {
        assert_eq!(
            header_refusal("join_storage", |h| put(h, 0, kind, 1))?,
            ProtocolError::field_value(512, "kind", kind)
        );
    }
    Ok(())
}

#[test]
fn test_the_address_is_utf8_and_names_a_carrier() -> TestResult {
    let storage = bytes("join_storage")?;
    let not_utf8 = with_section(&storage, 2, 0, |s| *s = b"tcp://\xFFhost".to_vec());
    assert_eq!(refusal(&not_utf8)?, ProtocolError::not_utf8(512, 0, 6));
    for address in [
        &b"http://host:1"[..],
        b"",
        b"tcp:/host",
        b"TCP://host",
        b"unix:/x",
    ] {
        let frame = with_section(&storage, 2, 0, |s| *s = address.to_vec());
        assert_eq!(
            refusal(&frame)?,
            ProtocolError::field_value(512, "address", address.len() as u64)
        );
    }
    for address in [&b"tcp://"[..], b"unix://", b"unix:///run/x.sock"] {
        let frame = with_section(&storage, 2, 0, |s| *s = address.to_vec());
        assert_eq!(
            JoinView::from_frame(&frame_of(&frame)?)?
                .address()
                .as_bytes(),
            address
        );
    }
    Ok(())
}

#[test]
fn test_partitions_are_whole_ascending_and_fit_the_kind() -> TestResult {
    let storage = bytes("join_storage")?;
    let ragged = with_section(&storage, 2, 1, |s| s.push(1));
    assert_eq!(
        refusal(&ragged)?,
        ProtocolError::section_length(512, 1, 13, 4)
    );
    // A bad address is found before a ragged partitions section.
    let both = with_section(&ragged, 2, 0, |s| *s = b"udp://x".to_vec());
    assert_eq!(
        refusal(&both)?,
        ProtocolError::field_value(512, "address", 7)
    );
    for (partitions, bad) in [
        (vec![0u32, 5], 0u64),
        (vec![2, 2], 2),
        (vec![5, 2], 2),
        (vec![2, 5, 9, 9], 9),
    ] {
        let frame = with_section(&storage, 2, 1, |s| {
            *s = partitions.iter().flat_map(|p| p.to_le_bytes()).collect();
        });
        assert_eq!(
            refusal(&frame)?,
            ProtocolError::field_value(512, "partition", bad)
        );
    }
    let empty = with_section(&storage, 2, 1, Vec::clear);
    assert_eq!(
        refusal(&empty)?,
        ProtocolError::count_mismatch(512, "partitions", 0, 1)
    );
    let compute = bytes("join_compute")?;
    let hosting = with_section(&compute, 2, 1, |s| *s = 7u32.to_le_bytes().to_vec());
    assert_eq!(
        refusal(&hosting)?,
        ProtocolError::count_mismatch(512, "partitions", 1, 0)
    );
    // A partition of u32::MAX is a partition, and one alone is ascending.
    let most = with_section(&storage, 2, 1, |s| *s = u32::MAX.to_le_bytes().to_vec());
    assert_eq!(
        JoinView::from_frame(&frame_of(&most)?)?.partition(0),
        Some(u32::MAX)
    );
    Ok(())
}
