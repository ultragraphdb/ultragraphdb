/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Welcome, WelcomeView};

use crate::utils::golden_bodies::{RELEASE, frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_welcome_reads_its_values() {
    let bytes = golden("welcome").bytes();
    let view = WelcomeView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.protocol(), 1);
    assert_eq!(view.release(), RELEASE);
    assert_eq!(view.max_frame(), 1_048_576);
    assert_eq!(view.max_streams(), 64);
    assert_eq!(view.initial_credit(), 4);
    assert_eq!(view.server(), "ultragraph 0.1");
    assert_eq!(
        view.to_body().unwrap(),
        Welcome::new(1, RELEASE, 1 << 20, 64, 4, "ultragraph 0.1")
    );
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
    assert_eq!(
        refused(|h| put(h, 0, 0, 2)),
        ProtocolErrorEnum::FieldValue {
            command: 2,
            field: "protocol",
            value: 0
        }
    );
    assert_eq!(
        refused(|h| put(h, 8, 4_095, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 2,
            field: "max_frame",
            value: 4_095
        }
    );
    assert_eq!(
        refused(|h| put(h, 8, 1_048_577, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 2,
            field: "max_frame",
            value: 1_048_577
        }
    );
    assert_eq!(
        refused(|h| put(h, 12, 0, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 2,
            field: "max_streams",
            value: 0
        }
    );
    // At the bounds, and a zero initial credit, are valid.
    assert!(accepted(|h| put(h, 8, 4_096, 4)));
    assert!(accepted(|h| put(h, 8, 1_048_576, 4)));
    assert!(accepted(|h| put(h, 12, 1, 4)));
    assert!(accepted(|h| put(h, 16, 0, 4)));
    // Bytes 2 and 3, between protocol and release, are unused.
    assert_eq!(
        refused(|h| h[2] = 1),
        ProtocolErrorEnum::ReservedNotZero { offset: 130 }
    );
}
