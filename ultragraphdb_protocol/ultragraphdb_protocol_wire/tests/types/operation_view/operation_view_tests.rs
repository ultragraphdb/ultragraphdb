/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, OperationView, RequestTermView};

use crate::types::operation::operation_tests::{counter, upsert};
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header, with_section};

#[test]
fn test_the_golden_operations_read_their_values() {
    let bytes = golden("operation").bytes();
    let view = OperationView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(
        (view.kind(), view.delta(), view.properties(), view.links()),
        (2, 0, 1, 1)
    );
    assert_eq!(
        view.terms().get(1),
        Some(RequestTermView::Literal {
            lexical: "x",
            datatype: None
        })
    );
    assert_eq!(view.to_body().unwrap(), upsert());
    let bytes = golden("operation_counter").bytes();
    let view = OperationView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!((view.kind(), view.delta()), (4, -5));
    assert_eq!(view.to_body().unwrap(), counter());
}

fn header_refused(name: &str, change: impl FnOnce(&mut [u8])) -> ProtocolErrorEnum {
    let bytes = with_command_header(&golden(name).bytes(), change);
    OperationView::from_frame(&frame_of(&bytes)).unwrap_err().0
}

#[test]
fn test_command_header_fields_fit_the_kind() {
    assert_eq!(
        header_refused("operation", |h| h[0] = 5),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "kind",
            value: 5
        }
    );
    assert_eq!(
        header_refused("operation", |h| h[0] = 0),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "kind",
            value: 0
        }
    );
    assert_eq!(
        header_refused("operation", |h| put(h, 8, 1, 8)),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "delta",
            value: 1
        }
    );
    assert_eq!(
        header_refused("operation", |h| h[0] = 1),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "links",
            value: 1
        }
    );
    assert_eq!(
        header_refused("operation_counter", |h| put(h, 16, 1, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "properties",
            value: 1
        }
    );
    // Compare-and-set, kind 3, takes no properties: the golden upsert's one property is refused before its link.
    assert_eq!(
        header_refused("operation", |h| h[0] = 3),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "properties",
            value: 1
        }
    );
    // A negative delta is a delta too: the counter's -5 under insert-if-absent, as its u64 bits 2^64 - 5.
    assert_eq!(
        header_refused("operation_counter", |h| h[0] = 1),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "delta",
            value: 18_446_744_073_709_551_611
        }
    );
    assert_eq!(
        header_refused("operation_counter", |h| put(h, 20, 2, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "links",
            value: 2
        }
    );
}

#[test]
fn test_args_are_counted_and_in_range() {
    fn refused(change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
        let bytes = with_section(&golden("operation").bytes(), 3, 0, change);
        OperationView::from_frame(&frame_of(&bytes)).unwrap_err().0
    }
    // graph, predicate, value, one property (2), one link (3): 8 args.
    assert_eq!(
        refused(|a| a.truncate(28)),
        ProtocolErrorEnum::CountMismatch {
            command: 9,
            field: "args",
            count: 7,
            expected: 8
        }
    );
    assert_eq!(
        refused(|a| put(a, 28, 2, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "link direction",
            value: 2
        }
    );
    assert_eq!(
        refused(|a| put(a, 4, 6, 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 9,
            section: 0,
            at: 1,
            index: 6,
            limit: 6
        }
    );
    assert_eq!(
        refused(|a| put(a, 4, u32::MAX.into(), 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 9,
            section: 0,
            at: 1,
            index: u32::MAX,
            limit: 6
        }
    );
}
