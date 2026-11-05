/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CatalogAnnouncement, EpochClose, EpochCloseView, MAX_EPOCH,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::epoch_close::epoch_close_tests::golden_epoch_close;
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the EpochCloseView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    EpochCloseView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

/// The golden close with section `index` changed by `change`, refused.
fn section_refusal(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolError> {
    refusal(&with_section(&bytes("epoch_close")?, 2, index, change))
}

#[test]
fn test_the_golden_close_reads_its_values() -> TestResult {
    let golden = bytes("epoch_close")?;
    let view = EpochCloseView::from_frame(&frame_of(&golden)?)?;
    assert_eq!(
        (view.epoch(), view.time(), view.more()),
        (42, 1_798_761_600_123_456, true)
    );
    assert_eq!(view.announcement_count(), 3);
    assert_eq!(view.announcement(0), Some(CatalogAnnouncement::new(3, 11)));
    assert_eq!(view.announcement(1), Some(CatalogAnnouncement::new(3, 12)));
    assert_eq!(view.announcement(2), Some(CatalogAnnouncement::new(7, 4)));
    assert_eq!(view.announcement(3), None);
    assert_eq!(view.eviction_count(), 2);
    assert_eq!(
        view.eviction(0),
        Some(std::array::from_fn(|i| 0x11 + i as u8))
    );
    assert_eq!(
        view.eviction(1),
        Some(std::array::from_fn(|i| 0x31 + i as u8))
    );
    assert_eq!(view.eviction(2), None);
    assert_eq!(view.placement(), 0);
    assert_eq!(view.to_body()?, golden_epoch_close());
    assert_eq!(view_any(&golden)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_close_with_nothing_and_the_last_epoch_reads() -> TestResult {
    let built = EpochClose::new(MAX_EPOCH, 0).frame(golden_role("epoch_close")?.header())?;
    let view = EpochCloseView::from_frame(&frame_of(&built)?)?;
    assert_eq!((view.epoch(), view.more()), (MAX_EPOCH, false));
    assert_eq!((view.announcement_count(), view.eviction_count()), (0, 0));
    assert_eq!(view.to_body()?, EpochClose::new(MAX_EPOCH, 0));
    Ok(())
}

#[test]
fn test_unused_bytes_epoch_and_more_are_checked_in_order() -> TestResult {
    assert_eq!(
        header_refusal("epoch_close", |h| put(h, 17, 1, 1))?,
        ProtocolError::reserved_not_zero(145)
    );
    assert_eq!(
        header_refusal("epoch_close", |h| {
            put(h, 0, 0, 8);
            put(h, 127, 1, 1);
        })?,
        ProtocolError::reserved_not_zero(255)
    );
    for epoch in [0, MAX_EPOCH + 1] {
        assert_eq!(
            header_refusal("epoch_close", |h| {
                put(h, 0, epoch, 8);
                put(h, 16, 2, 1);
            })?,
            ProtocolError::field_value(515, "epoch", epoch)
        );
    }
    for more in [2, 255] {
        assert_eq!(
            header_refusal("epoch_close", |h| put(h, 16, more, 1))?,
            ProtocolError::field_value(515, "more", more)
        );
    }
    // Bytes 24 to 31 are `placement`, read as any u64; byte 32 is unused again.
    for version in [1, 0x0102_0304_0506_0708, u64::MAX] {
        let placed = with_command_header(&bytes("epoch_close")?, |h| put(h, 24, version, 8));
        let view = EpochCloseView::from_frame(&frame_of(&placed)?)?;
        assert_eq!(view.placement(), version);
        assert_eq!(
            view.to_body()?,
            golden_epoch_close().with_placement(version)
        );
    }
    assert_eq!(
        header_refusal("epoch_close", |h| put(h, 32, 1, 1))?,
        ProtocolError::reserved_not_zero(160)
    );
    assert_eq!(
        header_refusal("epoch_close", |h| put(h, 23, 1, 1))?,
        ProtocolError::reserved_not_zero(151)
    );
    // Any time is a time.
    let time = with_command_header(&bytes("epoch_close")?, |h| put(h, 8, u64::MAX, 8));
    assert_eq!(
        EpochCloseView::from_frame(&frame_of(&time)?)?.time(),
        u64::MAX
    );
    Ok(())
}

#[test]
fn test_both_sections_are_whole_before_their_records() -> TestResult {
    assert_eq!(
        section_refusal(0, |s| s.truncate(15))?,
        ProtocolError::section_length(515, 0, 15, 16)
    );
    let golden = bytes("epoch_close")?;
    let both = with_section(&with_section(&golden, 2, 0, |s| s[8] = 0), 2, 1, |s| {
        s.push(1)
    });
    assert_eq!(
        refusal(&both)?,
        ProtocolError::section_length(515, 1, 33, 16)
    );
    Ok(())
}

#[test]
fn test_announcements_are_of_a_version_and_ordered() -> TestResult {
    let at = |i: usize, f: usize| 16 * i + 8 * f;
    assert_eq!(
        section_refusal(0, |s| s[at(0, 1)] = 0)?,
        ProtocolError::field_value(515, "announcement", 0)
    );
    // The same version again, a lower one in the same tenant, a lower tenant.
    assert_eq!(
        section_refusal(0, |s| s[at(1, 1)] = 11)?,
        ProtocolError::field_value(515, "announcement", 1)
    );
    assert_eq!(
        section_refusal(0, |s| s[at(1, 1)] = 10)?,
        ProtocolError::field_value(515, "announcement", 1)
    );
    assert_eq!(
        section_refusal(0, |s| s[at(2, 0)] = 2)?,
        ProtocolError::field_value(515, "announcement", 2)
    );
    // A higher tenant with a lower version is in order.
    let lower_version = with_section(&bytes("epoch_close")?, 2, 0, |s| s[at(2, 1)] = 1);
    assert_eq!(
        EpochCloseView::from_frame(&frame_of(&lower_version)?)?.announcement(2),
        Some(CatalogAnnouncement::new(7, 1))
    );
    Ok(())
}

#[test]
fn test_evictions_name_a_session_and_ascend() -> TestResult {
    assert_eq!(
        section_refusal(1, |s| s[..16].fill(0))?,
        ProtocolError::field_value(515, "eviction", 0)
    );
    assert_eq!(
        section_refusal(1, |s| {
            let first: Vec<u8> = s[..16].to_vec();
            s[16..].copy_from_slice(&first);
        })?,
        ProtocolError::field_value(515, "eviction", 1)
    );
    // Lower at its last byte alone.
    assert_eq!(
        section_refusal(1, |s| {
            let mut lower: Vec<u8> = s[..16].to_vec();
            lower[15] -= 1;
            s[16..].copy_from_slice(&lower);
        })?,
        ProtocolError::field_value(515, "eviction", 1)
    );
    // Higher at its last byte alone is in order.
    let higher = with_section(&bytes("epoch_close")?, 2, 1, |s| {
        let mut next: Vec<u8> = s[..16].to_vec();
        next[15] += 1;
        s[16..].copy_from_slice(&next);
    });
    assert_eq!(
        EpochCloseView::from_frame(&frame_of(&higher)?)?.eviction_count(),
        2
    );
    Ok(())
}
