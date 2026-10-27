/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, Describe, DescribeView};

use crate::utils::golden_bodies::{body_data, frame_of, golden, put, reframe, section};

/// A Describe answer whose labels are `labels`: its names list, sections 0 and 1, holds them in order.
fn describe(labels: &[&str]) -> Vec<u8> {
    Describe::new(0, 0)
        .with_labels(labels.iter().map(|l| l.to_string()).collect())
        .frame(golden("describe").header())
        .unwrap()
}

#[test]
fn test_a_list_reads_its_strings_in_order() {
    let bytes = describe(&["a", "", "ü", "<http://ex.org/x>"]);
    let view = DescribeView::from_frame(&frame_of(&bytes)).unwrap();
    let list = view.strings();
    assert_eq!(list.len(), 4);
    assert!(!list.is_empty());
    assert_eq!(list.get(0), Some("a"));
    assert_eq!(list.get(1), Some(""));
    assert_eq!(list.get(2), Some("ü"));
    assert_eq!(list.get(3), Some("<http://ex.org/x>"));
    assert_eq!(list.get(4), None);
    assert_eq!(
        list.iter().collect::<Vec<_>>(),
        vec!["a", "", "ü", "<http://ex.org/x>"]
    );
    let empty = describe(&[]);
    let view = DescribeView::from_frame(&frame_of(&empty)).unwrap();
    assert!(view.strings().is_empty());
    assert_eq!(view.strings().iter().count(), 0);
}

fn refused(labels: &[&str], change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
    let bytes = describe(labels);
    let mut data = body_data(&bytes).to_vec();
    change(&mut data);
    DescribeView::from_frame(&frame_of(&reframe(&bytes, &data)))
        .unwrap_err()
        .0
}

#[test]
fn test_ends_are_whole_monotone_and_reach_the_text_end() {
    // Two strings "ab" and "cd": ends 2, 4.
    let ends_at = |data: &[u8]| section(data, 0, 0).0;
    assert_eq!(
        refused(&["ab", "cd"], |d| {
            let at = ends_at(d);
            put(d, at, 3, 4);
            put(d, at + 4, 2, 4);
        }),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 12,
            section: 0,
            at: 1,
            index: 2,
            limit: 4
        }
    );
    assert_eq!(
        refused(&["ab", "cd"], |d| {
            let at = ends_at(d);
            put(d, at + 4, 5, 4);
        }),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 12,
            section: 0,
            at: 1,
            index: 5,
            limit: 4
        }
    );
    assert_eq!(
        refused(&["ab", "cd"], |d| {
            let at = ends_at(d);
            put(d, at + 4, 3, 4);
        }),
        ProtocolErrorEnum::CountMismatch {
            command: 12,
            field: "string text",
            count: 4,
            expected: 3
        }
    );
    // Ends of 7 bytes: not whole u32s.
    assert_eq!(
        refused(&["ab", "cd"], |d| put(d, 4, 7, 4)),
        ProtocolErrorEnum::SectionLength {
            command: 12,
            section: 0,
            len: 7,
            unit: 4
        }
    );
}

#[test]
fn test_each_string_must_be_utf8() {
    // The text must be UTF-8 and every end on a character boundary; the error names the first bad byte or the end.
    assert_eq!(
        refused(&["ab", "cd"], |d| {
            let text = section(d, 0, 1).0;
            d[text + 2] = 0x80;
        }),
        ProtocolErrorEnum::NotUtf8 {
            command: 12,
            section: 1,
            offset: 2
        }
    );
    // A two-byte character split across two strings: the text is UTF-8, but the first end is inside "ü".
    let bytes = describe(&["x", "yz"]);
    let mut d = body_data(&bytes).to_vec();
    let text = section(&d, 0, 1).0;
    d[text..text + 3].copy_from_slice("üa".as_bytes());
    assert_eq!(
        DescribeView::from_frame(&frame_of(&reframe(&bytes, &d)))
            .unwrap_err()
            .0,
        ProtocolErrorEnum::NotUtf8 {
            command: 12,
            section: 1,
            offset: 1
        }
    );
}
