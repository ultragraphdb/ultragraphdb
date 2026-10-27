/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, Execute, ExecuteView};

use crate::utils::golden_bodies::{frame_of, golden, put, reframe, section};

fn execute(parameters: &[&str]) -> Vec<u8> {
    Execute::new(
        1,
        0,
        parameters.iter().map(|p| p.to_string()).collect(),
        Vec::new(),
    )
    .frame(golden("execute").header())
    .unwrap()
}

#[test]
fn test_a_list_reads_its_strings_in_order() {
    let bytes = execute(&["a", "", "ü", "<http://ex.org/x>"]);
    let view = ExecuteView::from_frame(&frame_of(&bytes)).unwrap();
    let list = view.parameters();
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
    let empty = execute(&[]);
    let view = ExecuteView::from_frame(&frame_of(&empty)).unwrap();
    assert!(view.parameters().is_empty());
    assert_eq!(view.parameters().iter().count(), 0);
}

fn refused(parameters: &[&str], change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
    let bytes = execute(parameters);
    let mut data = crate::utils::golden_bodies::body_data(&bytes).to_vec();
    change(&mut data);
    ExecuteView::from_frame(&frame_of(&reframe(&bytes, &data)))
        .unwrap_err()
        .0
}

#[test]
fn test_ends_are_whole_monotone_and_reach_the_text_end() {
    // Sections: 0 ends, 1 text, 2 resume. Two strings "ab" and "cd": ends 2, 4.
    let ends_at = |data: &[u8]| section(data, 0, 0).0;
    assert_eq!(
        refused(&["ab", "cd"], |d| {
            let at = ends_at(d);
            put(d, at, 3, 4);
            put(d, at + 4, 2, 4);
        }),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
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
            command: 11,
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
            command: 11,
            field: "string text",
            count: 4,
            expected: 3
        }
    );
    // Ends of 7 bytes: not whole u32s.
    assert_eq!(
        refused(&["ab", "cd"], |d| put(d, 4, 7, 4)),
        ProtocolErrorEnum::SectionLength {
            command: 11,
            section: 0,
            len: 7,
            unit: 4
        }
    );
}

#[test]
fn test_each_string_must_be_utf8() {
    // The text must be UTF-8 and every end on a character boundary; the error names the first bad byte or the end.
    // The second string's first byte is a continuation byte: the text is invalid at offset 2.
    assert_eq!(
        refused(&["ab", "cd"], |d| {
            let text = section(d, 0, 1).0;
            d[text + 2] = 0x80;
        }),
        ProtocolErrorEnum::NotUtf8 {
            command: 11,
            section: 1,
            offset: 2
        }
    );
    // A two-byte character split across two strings: the text is UTF-8, but the first end is inside "ü".
    let mut two = execute(&["x", "yz"]);
    let mut d = crate::utils::golden_bodies::body_data(&two).to_vec();
    let text = section(&d, 0, 1).0;
    d[text..text + 3].copy_from_slice("üa".as_bytes());
    two = reframe(&two, &d);
    assert_eq!(
        ExecuteView::from_frame(&frame_of(&two)).unwrap_err().0,
        ProtocolErrorEnum::NotUtf8 {
            command: 11,
            section: 1,
            offset: 1
        }
    );
}
