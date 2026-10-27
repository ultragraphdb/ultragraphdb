/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Credit, CreditView};

use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_credit_reads_its_frames() {
    let bytes = golden("credit").bytes();
    let view = CreditView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.frames(), 2);
    assert_eq!(view.to_body().unwrap(), Credit::new(2));
    let most = with_command_header(&bytes, |h| put(h, 0, u32::MAX.into(), 4));
    assert_eq!(
        CreditView::from_frame(&frame_of(&most)).unwrap().frames(),
        u32::MAX
    );
}

#[test]
fn test_a_grant_of_nothing_is_refused() {
    let zero = with_command_header(&golden("credit").bytes(), |h| put(h, 0, 0, 4));
    assert_eq!(
        CreditView::from_frame(&frame_of(&zero)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 14,
            field: "frames",
            value: 0
        }
    );
}
