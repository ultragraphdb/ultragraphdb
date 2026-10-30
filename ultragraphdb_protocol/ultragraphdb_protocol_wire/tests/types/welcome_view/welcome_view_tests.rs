/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Welcome, WelcomeView};

use crate::utils::golden_bodies::{
    RELEASE, frame_of, golden, join_sections, put, reframe, split_sections, with_command_header,
};

#[test]
fn test_the_golden_welcome_reads_its_values() {
    let bytes = golden("welcome").bytes();
    let view = WelcomeView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.protocol(), 1);
    assert!(view.authenticates());
    assert_eq!(view.release(), RELEASE);
    assert_eq!(view.max_frame(), 1_048_576);
    assert_eq!(view.max_streams(), 64);
    assert_eq!(view.initial_credit(), 4);
    assert_eq!(view.max_write_set(), 1_040_384);
    assert_eq!(view.transaction_ms(), 10_000);
    assert_eq!(view.server(), "ultragraph 0.1");
    assert_eq!(
        view.mechanisms().iter().collect::<Vec<_>>(),
        ["SCRAM-SHA-256"]
    );
    assert_eq!(
        view.to_body().unwrap(),
        Welcome::new(1, RELEASE, "ultragraph 0.1")
            .with_max_streams(64)
            .with_mechanisms(["SCRAM-SHA-256"])
    );
    let bytes = golden("welcome_no_auth").bytes();
    let view = WelcomeView::from_frame(&frame_of(&bytes)).unwrap();
    assert!(!view.authenticates());
    assert!(view.mechanisms().is_empty());
    assert_eq!(view.max_frame(), 524_288);
}

fn refused(change: impl FnOnce(&mut [u8])) -> ProtocolErrorEnum {
    let bytes = with_command_header(&golden("welcome").bytes(), change);
    WelcomeView::from_frame(&frame_of(&bytes)).unwrap_err().0
}

fn accepted(change: impl FnOnce(&mut [u8])) -> bool {
    let bytes = with_command_header(&golden("welcome").bytes(), change);
    WelcomeView::from_frame(&frame_of(&bytes)).is_ok()
}

#[test]
fn test_welcome_fields_are_bounded() {
    let field = |field: &'static str, value: u64| ProtocolErrorEnum::FieldValue {
        command: 2,
        field,
        value,
    };
    assert_eq!(refused(|h| put(h, 0, 0, 2)), field("protocol", 0));
    assert_eq!(refused(|h| h[2] = 2), field("auth", 2));
    assert_eq!(refused(|h| put(h, 8, 4_096, 4)), field("max_frame", 4_096));
    assert_eq!(
        refused(|h| put(h, 8, 524_287, 4)),
        field("max_frame", 524_287)
    );
    assert_eq!(
        refused(|h| put(h, 8, 1_048_577, 4)),
        field("max_frame", 1_048_577)
    );
    assert_eq!(refused(|h| put(h, 12, 0, 4)), field("max_streams", 0));
    // At the bounds, and a zero initial credit, write set and transaction limit, are valid.
    assert!(accepted(|h| put(h, 8, 524_288, 4)));
    assert!(accepted(|h| put(h, 8, 1_048_576, 4)));
    assert!(accepted(|h| put(h, 12, 1, 4)));
    assert!(accepted(|h| put(h, 16, 0, 4)));
    assert!(accepted(|h| put(h, 20, 0, 4)));
    assert!(accepted(|h| put(h, 24, u64::from(u32::MAX), 4)));
}

#[test]
fn test_unused_bytes_are_checked_before_the_fields() {
    // Byte 3 sits between `auth` and `release`; bytes 28 to 127 follow `transaction_ms`.
    for at in [3usize, 28, 100, 127] {
        assert_eq!(
            refused(|h| {
                h[at] = 1;
                put(h, 0, 0, 2);
            }),
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    // Then the fields in their order: protocol, auth, max_frame, max_streams.
    assert!(matches!(
        refused(|h| {
            put(h, 0, 0, 2);
            h[2] = 9;
        }),
        ProtocolErrorEnum::FieldValue {
            field: "protocol",
            ..
        }
    ));
    assert!(matches!(
        refused(|h| {
            h[2] = 9;
            put(h, 8, 0, 4);
        }),
        ProtocolErrorEnum::FieldValue { field: "auth", .. }
    ));
    assert!(matches!(
        refused(|h| {
            put(h, 8, 0, 4);
            put(h, 12, 0, 4);
        }),
        ProtocolErrorEnum::FieldValue {
            field: "max_frame",
            ..
        }
    ));
}

#[test]
fn test_auth_and_the_mechanisms_agree() {
    // auth 1 with no mechanism, and auth 0 with one, are refused.
    let bytes = golden("welcome").bytes();
    let mut secs = split_sections(&golden("welcome").data(), 0, 3);
    secs[1].clear();
    secs[2].clear();
    let none = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        WelcomeView::from_frame(&frame_of(&none)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 2,
            field: "mechanisms",
            count: 0,
            expected: 1
        }
    );
    let open = with_command_header(&bytes, |h| h[2] = 0);
    assert_eq!(
        WelcomeView::from_frame(&frame_of(&open)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 2,
            field: "mechanisms",
            count: 1,
            expected: 0
        }
    );
    // The mechanisms are a string list: an end past the text is refused by the list's rule.
    let mut secs = split_sections(&golden("welcome").data(), 0, 3);
    secs[1] = 99u32.to_le_bytes().to_vec();
    let past = reframe(&bytes, &join_sections(&[], &secs));
    assert!(matches!(
        WelcomeView::from_frame(&frame_of(&past)).unwrap_err().0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 2,
            section: 1,
            ..
        }
    ));
}
