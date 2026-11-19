/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyView, MAX_EPOCH, RoleKind, Subscribe, SubscribeView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::subscribe::subscribe_tests::golden_subscribe;
use crate::utils::golden_bodies::{frame_of, put, reframe, view_any, with_command_header};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the SubscribeView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    SubscribeView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

#[test]
fn test_the_golden_subscribe_reads_its_epoch() -> TestResult {
    let golden = bytes("subscribe")?;
    let view = SubscribeView::from_frame(&frame_of(&golden)?)?;
    assert_eq!(view.from(), 41);
    assert_eq!(view.to_body()?, golden_subscribe());
    assert_eq!(view_any(&golden)?, Ok(()));
    for from in [0, MAX_EPOCH] {
        let frame = with_command_header(&golden, |h| put(h, 0, from, 8));
        let second = SubscribeView::from_frame(&frame_of(&frame)?)?;
        assert_eq!(second.to_body()?, Subscribe::new(from));
    }
    Ok(())
}

#[test]
fn test_unused_bytes_the_epoch_and_the_sections_are_checked() -> TestResult {
    assert_eq!(
        header_refusal("subscribe", |h| put(h, 9, 1, 1))?,
        ProtocolError::reserved_not_zero(137)
    );
    for from in [MAX_EPOCH + 1, u64::MAX] {
        assert_eq!(
            header_refusal("subscribe", |h| put(h, 0, from, 8))?,
            ProtocolError::field_value(514, "from", from)
        );
    }
    let golden = bytes("subscribe")?;
    assert_eq!(
        refusal(&reframe(&golden, &[0; 8]))?,
        ProtocolError::trailing_bytes(514, 0, 8)
    );
    Ok(())
}

/// C1.97: the Order line of `PROTOCOL.md` §13.3: unused bytes 9–127 zero; `from` at most 2⁴² − 1; `latest` 0 or 1, and
/// 1 only with `from` 0; sections.
#[test]
fn test_latest_is_a_boolean_that_names_no_epoch() -> TestResult {
    assert_eq!(
        header_refusal("subscribe", |h| {
            put(h, 127, 1, 1);
            put(h, 8, 2, 1);
        })?,
        ProtocolError::reserved_not_zero(255)
    );
    for latest in [2u64, 255] {
        assert_eq!(
            header_refusal("subscribe", |h| {
                put(h, 0, 0, 8);
                put(h, 8, latest, 1);
            })?,
            ProtocolError::field_value(514, "latest", latest)
        );
    }
    for from in [1u64, 5, 41, MAX_EPOCH] {
        assert_eq!(
            header_refusal("subscribe", |h| {
                put(h, 0, from, 8);
                put(h, 8, 1, 1);
            })?,
            ProtocolError::field_value(514, "latest", 1),
            "from {from}"
        );
    }
    // `from` is checked before `latest`.
    assert_eq!(
        header_refusal("subscribe", |h| {
            put(h, 0, MAX_EPOCH + 1, 8);
            put(h, 8, 2, 1);
        })?,
        ProtocolError::field_value(514, "from", MAX_EPOCH + 1)
    );
    // `latest` is checked before the sections.
    let with_body = reframe(
        &with_command_header(&bytes("subscribe")?, |h| put(h, 8, 2, 1)),
        &[0; 8],
    );
    assert_eq!(
        refusal(&with_body)?,
        ProtocolError::field_value(514, "latest", 2)
    );
    Ok(())
}

/// C1.98: `latest` 1 with `from` 0 reads, as `latest` 0 does with any epoch; the constructor in use builds `latest` 0.
#[test]
fn test_a_subscribe_from_the_latest_close_reads() -> TestResult {
    let latest = with_command_header(&bytes("subscribe")?, |h| {
        put(h, 0, 0, 8);
        put(h, 8, 1, 1);
    });
    let view = SubscribeView::from_frame(&frame_of(&latest)?)?;
    assert_eq!((view.from(), view.latest()), (0, true));
    assert_eq!(view.to_body()?, Subscribe::at_latest());
    assert_eq!(view_any(&latest)?, Ok(()));
    for from in [0, 7, MAX_EPOCH] {
        let frame = with_command_header(&bytes("subscribe")?, |h| put(h, 0, from, 8));
        let second = SubscribeView::from_frame(&frame_of(&frame)?)?;
        assert_eq!((second.from(), second.latest()), (from, false));
        assert!(!second.to_body()?.latest());
    }
    Ok(())
}

/// `PROTOCOL.md` §13.3, wire-protocol "A Subscribe from the latest close names no epoch": compute alone sends `latest` 1, a
/// rule of the connection's role, which the receiving session checks through the view.
#[test]
fn test_only_a_compute_nodes_connection_asks_from_the_latest_close() -> TestResult {
    let latest = with_command_header(&bytes("subscribe")?, |h| {
        put(h, 0, 0, 8);
        put(h, 8, 1, 1);
    });
    let view = SubscribeView::from_frame(&frame_of(&latest)?)?;
    view.check_sender(RoleKind::Compute)?;
    for sender in [RoleKind::Storage, RoleKind::Frontend] {
        assert_eq!(
            view.check_sender(sender).err(),
            Some(ProtocolError::field_value(514, "latest", 1)),
            "{sender:?}"
        );
    }
    let golden = bytes("subscribe")?;
    let from_an_epoch = SubscribeView::from_frame(&frame_of(&golden)?)?;
    for sender in [RoleKind::Frontend, RoleKind::Storage, RoleKind::Compute] {
        from_an_epoch.check_sender(sender)?;
    }
    Ok(())
}
