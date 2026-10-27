/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Hello, HelloView};

use crate::utils::golden_bodies::{RELEASE, frame_of, golden, put, reframe, with_command_header};

#[test]
fn test_the_golden_hello_reads_its_values() {
    let bytes = golden("hello").bytes();
    let view = HelloView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.protocol_min(), 1);
    assert_eq!(view.protocol_max(), 1);
    assert_eq!(view.release(), RELEASE);
    assert_eq!(view.client(), "ultragraph-test");
    assert_eq!(
        view.to_body().unwrap(),
        Hello::new(1, 1, RELEASE, "ultragraph-test")
    );
}

#[test]
fn test_a_protocol_range_must_start_at_one_and_not_run_backwards() {
    let bytes = golden("hello").bytes();
    let zero = with_command_header(&bytes, |h| put(h, 0, 0, 2));
    assert_eq!(
        HelloView::from_frame(&frame_of(&zero)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "protocol_min",
            value: 0
        }
    );
    let backwards = with_command_header(&bytes, |h| {
        put(h, 0, 3, 2);
        put(h, 2, 2, 2);
    });
    assert_eq!(
        HelloView::from_frame(&frame_of(&backwards)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "protocol_max",
            value: 2
        }
    );
    // A range of one protocol above 1 is valid.
    let high = with_command_header(&bytes, |h| {
        put(h, 0, 3, 2);
        put(h, 2, 3, 2);
    });
    assert_eq!(
        HelloView::from_frame(&frame_of(&high))
            .unwrap()
            .protocol_min(),
        3
    );
}

#[test]
fn test_a_name_that_is_not_utf8_is_refused_at_its_first_bad_byte() {
    let bytes = golden("hello").bytes();
    let mut data = golden("hello").data();
    // The name starts at 8, after the table of one section: byte 3 of it.
    data[8 + 3] = 0xFF;
    assert_eq!(
        HelloView::from_frame(&frame_of(&reframe(&bytes, &data)))
            .unwrap_err()
            .0,
        ProtocolErrorEnum::NotUtf8 {
            command: 1,
            section: 0,
            offset: 3
        }
    );
    // A name of multi-byte characters reads.
    let unicode = Hello::new(1, 1, 0, "ültra → graph");
    use ultragraphdb_protocol_wire::BodyBuilder;
    let frame = unicode.frame(golden("hello").header()).unwrap();
    assert_eq!(
        HelloView::from_frame(&frame_of(&frame)).unwrap().client(),
        "ültra → graph"
    );
}
