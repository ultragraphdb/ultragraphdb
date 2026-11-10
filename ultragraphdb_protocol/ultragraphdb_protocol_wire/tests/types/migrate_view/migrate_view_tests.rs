/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Migrate's Order (`PROTOCOL.md` §8.6) over frames laid out by hand.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyView, Command, MAX_EPOCH, MigrateAction, MigrateState, MigrateView,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::frame_of;
use crate::utils::ugs_frames::{frame, outcome, refused};

fn field_value(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 25,
        field,
        value,
    }
}

/// A Migrate of tenant 2 naming `staged`, `action`, `state`, `refusal` and `epoch`, its message and report.
fn migrate(
    staged: u64,
    action_state: [u8; 2],
    refusal: u32,
    epoch: u64,
    sections: [&[u8]; 2],
) -> TestResult<Vec<u8>> {
    frame(
        Command::Migrate,
        &[
            (0, &2u64.to_le_bytes()),
            (8, &staged.to_le_bytes()),
            (16, &action_state),
            (20, &refusal.to_le_bytes()),
            (24, &epoch.to_le_bytes()),
        ],
        &[sections[0].to_vec(), sections[1].to_vec()],
        true,
    )
}

/// wire-protocol, "Purge and Migrate decode and are refused by stage": a well-formed Migrate decodes, request and
/// reply.
#[test]
fn test_a_well_formed_migrate_decodes() -> TestResult {
    let request = migrate(5, [1, 0], 0, 0, [b"", b""])?;
    let view = MigrateView::from_frame(&frame_of(&request)?)?;
    assert_eq!(
        (view.tenant(), view.staged(), view.action()),
        (2, 5, MigrateAction::Start)
    );
    assert_eq!((view.state(), view.refusal(), view.epoch()), (None, 0, 0));
    let failed = migrate(5, [5, 9], 2_299, 31, [b"UGS2299 m", b"r"])?;
    let view = MigrateView::from_frame(&frame_of(&failed)?)?;
    assert_eq!(view.state(), Some(MigrateState::Failed));
    assert_eq!((view.refusal(), view.epoch()), (2_299, 31));
    assert_eq!((view.message(), view.report()), ("UGS2299 m", "r"));
    let body = view.to_body()?;
    assert_eq!(
        (body.staged(), body.refusal(), body.message()),
        (5, 2_299, "UGS2299 m")
    );
    Ok(())
}

/// `staged` at least 1; `action` 1 to 5; `state` 0 in a request and 1 to 9 in a reply.
#[test]
fn test_staged_action_and_state_are_checked() -> TestResult {
    assert_eq!(
        refused::<MigrateView>(&migrate(0, [1, 0], 0, 0, [b"", b""])?)?,
        field_value("staged", 0)
    );
    for action in [0u8, 6] {
        assert_eq!(
            refused::<MigrateView>(&migrate(5, [action, 0], 0, 0, [b"", b""])?)?,
            field_value("action", action.into())
        );
    }
    for action in 1u8..=5 {
        assert_eq!(
            outcome::<MigrateView>(&migrate(5, [action, 0], 0, 0, [b"", b""])?)?,
            None
        );
    }
    assert_eq!(
        refused::<MigrateView>(&migrate(5, [5, 10], 0, 30, [b"", b""])?)?,
        field_value("state", 10)
    );
    for state in 1u8..=8 {
        assert_eq!(
            outcome::<MigrateView>(&migrate(5, [5, state], 0, 30, [b"", b""])?)?,
            None
        );
    }
    Ok(())
}

/// `refusal` is 0 unless `state` is 9, and then 0, a cancelled run, or 2201 to 2299.
#[test]
fn test_a_refusal_names_a_failed_runs_ugs_number() -> TestResult {
    assert_eq!(
        refused::<MigrateView>(&migrate(5, [5, 4], 2_201, 30, [b"m", b""])?)?,
        field_value("refusal", 2_201)
    );
    for refusal in [2_200u32, 2_300] {
        assert_eq!(
            refused::<MigrateView>(&migrate(5, [5, 9], refusal, 30, [b"m", b""])?)?,
            field_value("refusal", refusal.into())
        );
    }
    for refusal in [2_201u32, 2_299] {
        assert_eq!(
            outcome::<MigrateView>(&migrate(5, [5, 9], refusal, 30, [b"m", b""])?)?,
            None
        );
    }
    assert_eq!(
        outcome::<MigrateView>(&migrate(5, [4, 9], 0, 30, [b"", b""])?)?,
        None
    );
    Ok(())
}

/// `epoch` at most 2⁴² − 1; a request's sections empty; both sections UTF-8; the message empty when `refusal` is 0.
#[test]
fn test_epoch_and_sections_are_checked() -> TestResult {
    assert_eq!(
        refused::<MigrateView>(&migrate(5, [5, 2], 0, MAX_EPOCH + 1, [b"", b""])?)?,
        field_value("epoch", MAX_EPOCH + 1)
    );
    assert_eq!(
        outcome::<MigrateView>(&migrate(5, [5, 2], 0, MAX_EPOCH, [b"", b""])?)?,
        None
    );
    for sections in [[&b"m"[..], &b""[..]], [&b""[..], &b"r"[..]]] {
        let refusal = refused::<MigrateView>(&migrate(5, [1, 0], 0, 0, sections)?)?;
        assert!(
            matches!(
                refusal,
                ProtocolErrorEnum::CountMismatch {
                    command: 25,
                    field: "migrate",
                    ..
                }
            ),
            "{refusal:?}"
        );
    }
    assert_eq!(
        refused::<MigrateView>(&migrate(5, [5, 9], 2_201, 30, [&[0xFF], b""])?)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 25,
            section: 0,
            offset: 0
        }
    );
    assert_eq!(
        refused::<MigrateView>(&migrate(5, [5, 2], 0, 30, [b"", &[b'r', 0xFF]])?)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 25,
            section: 1,
            offset: 1
        }
    );
    let refusal = refused::<MigrateView>(&migrate(5, [5, 2], 0, 30, [b"m", b""])?)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::CountMismatch {
                command: 25,
                field: "message",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// `epoch` is 0 in a request (§8.6's command header): a request naming one is refused, so a body read back builds the
/// frame it was read from.
#[test]
fn test_a_request_names_no_epoch() -> TestResult {
    for epoch in [1, MAX_EPOCH] {
        assert_eq!(
            refused::<MigrateView>(&migrate(5, [1, 0], 0, epoch, [b"", b""])?)?,
            field_value("epoch", epoch)
        );
    }
    Ok(())
}

/// The command header's bytes 18, 19 and 32 to 127 are zero.
#[test]
fn test_unused_bytes_are_zero() -> TestResult {
    let bytes = migrate(5, [1, 0], 0, 0, [b"", b""])?;
    for at in [18usize, 19, 32, 127] {
        let changed =
            crate::utils::golden_bodies::with_command_header(&bytes, |header| header[at] = 1);
        assert_eq!(
            refused::<MigrateView>(&changed)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}
