/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Hello, HelloView, MAX_FRAME_SIZE};

use crate::utils::golden_bodies::{RELEASE, frame_of, golden, put, reframe, with_command_header};

#[test]
fn test_the_golden_hello_reads_its_values() {
    let bytes = golden("hello").bytes();
    let view = HelloView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.protocol_min(), 1);
    assert_eq!(view.protocol_max(), 1);
    assert_eq!(view.release(), RELEASE);
    assert_eq!(view.max_frame(), 1_048_576);
    assert_eq!(view.client(), "ultragraph-test");
    assert_eq!(
        view.to_body().unwrap(),
        Hello::new(1, 1, RELEASE, MAX_FRAME_SIZE as u32, "ultragraph-test")
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
fn test_the_largest_frame_a_client_takes_is_from_the_floor_to_one_mebibyte() {
    let bytes = golden("hello").bytes();
    let at = |max_frame: u64| {
        let frame = with_command_header(&bytes, |h| put(h, 8, max_frame, 4));
        HelloView::from_frame(&frame_of(&frame))
            .map(|v| v.max_frame())
            .map_err(|e| e.0)
    };
    for max_frame in [524_288u64, 524_289, 1_048_575, 1_048_576] {
        assert_eq!(at(max_frame), Ok(max_frame as u32));
    }
    for max_frame in [0u64, 4_096, 524_287, 1_048_577, u64::from(u32::MAX)] {
        assert_eq!(
            at(max_frame),
            Err(ProtocolErrorEnum::FieldValue {
                command: 1,
                field: "max_frame",
                value: max_frame
            })
        );
    }
}

#[test]
fn test_unused_bytes_are_checked_before_the_fields() {
    let bytes = golden("hello").bytes();
    for at in [12usize, 64, 127] {
        let frame = with_command_header(&bytes, |h| {
            h[at] = 1;
            put(h, 0, 0, 2);
        });
        assert_eq!(
            HelloView::from_frame(&frame_of(&frame)).unwrap_err().0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    // max_frame is checked after the protocol range.
    let frame = with_command_header(&bytes, |h| {
        put(h, 0, 0, 2);
        put(h, 8, 0, 4);
    });
    assert!(matches!(
        HelloView::from_frame(&frame_of(&frame)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            field: "protocol_min",
            ..
        }
    ));
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
    let unicode = Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "ültra → graph");
    use ultragraphdb_protocol_wire::BodyBuilder;
    let frame = unicode.frame(golden("hello").header()).unwrap();
    assert_eq!(
        HelloView::from_frame(&frame_of(&frame)).unwrap().client(),
        "ültra → graph"
    );
}
