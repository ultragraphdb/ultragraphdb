/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conformance suite's malformed frames, over every golden body: a case per rule of the body layout, every
//! truncation, every bit flip of the body and of the command header, unused command-header bytes, and a view asked of
//! another command's frame. Each is refused with its `ProtocolError`, and none panics.

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyView, CreditView, FrameView, HelloView, RoundView, WelcomeView,
};

use crate::utils::golden_bodies::{
    BODIES, CLUSTER, GoldenBody, body_data, golden, put, reframe, section, u32_at, view_any,
    with_command, with_command_header,
};

/// The sections each command's body has, as its layout table states.
fn sections_of(command: u16) -> usize {
    match command {
        1 | 2 => 1,
        3 | 4 | 6 | 14 | 15 => 0,
        5 => 1,
        7 => 4,
        8 => 6,
        9 => 3,
        10 => 5,
        11 => 4,
        12 => 7,
        13 => 7,
        16 => 2,
        17 => 3,
        _ => unreachable!("no such command"),
    }
}

/// The command-header bytes each command's fields use, as its layout table states.
fn used(command: u16) -> Vec<(usize, usize)> {
    match command {
        1 => vec![(0, 8)],
        2 => vec![(0, 2), (4, 20)],
        3 => vec![(0, 8)],
        4 => vec![(0, 17)],
        5 => vec![(0, 24)],
        6 | 15 => vec![],
        7 => vec![(0, 12)],
        8 => vec![(0, 8)],
        9 => vec![(0, 1), (8, 24)],
        10 => vec![(0, 24)],
        11 => vec![(0, 12)],
        12 => vec![(0, 16)],
        13 => vec![(0, 8)],
        14 => vec![(0, 4)],
        16 => vec![(0, 2), (8, 16)],
        17 => vec![(0, 36)],
        _ => unreachable!("no such command"),
    }
}

fn refusal(bytes: &[u8]) -> ProtocolErrorEnum {
    view_any(bytes).expect_err("the frame is refused").0
}

#[test]
fn test_every_golden_body_reads_and_rebuilds() {
    for body in BODIES {
        let bytes = body.bytes();
        assert_eq!(bytes.len(), body.size as usize, "{}", body.name);
        let frame = FrameView::validate(&bytes, CLUSTER).expect("a golden frame validates");
        assert_eq!(frame.command().number(), body.command, "{}", body.name);
        assert_eq!(frame.header().checksum(), body.checksum, "{}", body.name);
        assert_eq!(
            frame.header().checksum_body(),
            body.checksum_body,
            "{}",
            body.name
        );
        assert_eq!(view_any(&bytes), Ok(()), "{}", body.name);
    }
}

#[test]
fn test_every_truncation_of_every_body_is_refused() {
    for body in BODIES {
        let bytes = body.bytes();
        let data = body_data(&bytes);
        for len in 0..data.len() {
            let truncated = reframe(&bytes, &data[..len]);
            assert!(view_any(&truncated).is_err(), "{} at {len}", body.name);
        }
    }
}

#[test]
fn test_every_bit_flip_of_every_body_reads_or_is_refused_without_a_panic() {
    for body in BODIES {
        let bytes = body.bytes();
        let data = body_data(&bytes).to_vec();
        for bit in 0..data.len() * 8 {
            let mut flipped = data.clone();
            flipped[bit / 8] ^= 1 << (bit % 8);
            // A flip may give another valid body, a term's text changed; it must never panic.
            let _ = view_any(&reframe(&bytes, &flipped));
        }
    }
}

#[test]
fn test_every_bit_flip_of_every_command_header_reads_or_is_refused_without_a_panic() {
    for body in BODIES {
        let bytes = body.bytes();
        for bit in 0..128 * 8 {
            let flipped = with_command_header(&bytes, |h| h[bit / 8] ^= 1 << (bit % 8));
            let _ = view_any(&flipped);
        }
    }
}

#[test]
fn test_each_unused_command_header_byte_must_be_zero() {
    for body in BODIES {
        let bytes = body.bytes();
        let used = used(body.command);
        for at in (0..128).filter(|at| !used.iter().any(|(start, end)| (*start..*end).contains(at)))
        {
            let changed = with_command_header(&bytes, |h| h[at] = 0x40);
            assert_eq!(
                refusal(&changed),
                ProtocolErrorEnum::ReservedNotZero { offset: 128 + at },
                "{} at {at}",
                body.name
            );
        }
    }
}

fn layout_cases(body: GoldenBody) -> Vec<(String, Vec<u8>, ProtocolErrorEnum)> {
    let bytes = body.bytes();
    let data = body_data(&bytes).to_vec();
    let n = sections_of(body.command);
    let command = body.command;
    let mut cases = Vec::new();
    // Bytes past the last section.
    let mut longer = data.clone();
    longer.push(0);
    cases.push((
        "one byte past the end".to_string(),
        longer,
        ProtocolErrorEnum::TrailingBytes {
            command,
            end: data.len(),
            len: data.len() + 1,
        },
    ));
    if n == 0 {
        return cases;
    }
    // A body shorter than its section table.
    cases.push((
        "shorter than the table".to_string(),
        data[..7].to_vec(),
        ProtocolErrorEnum::BodyTooShort {
            command,
            len: 7,
            need: 8 * n,
        },
    ));
    // Section 0's offset past the body.
    let mut past = data.clone();
    let past_at = (data.len() + 8) as u32;
    put(&mut past, 0, past_at.into(), 4);
    cases.push((
        "an offset past the body".to_string(),
        past,
        ProtocolErrorEnum::SectionOutOfBounds {
            command,
            section: 0,
            offset: past_at,
            len: section(&data, 0, 0).1 as u32,
            body: data.len(),
        },
    ));
    // A length past the body.
    let mut long = data.clone();
    let (offset, len) = section(&data, 0, n - 1);
    put(
        &mut long,
        8 * (n - 1) + 4,
        (data.len() - offset + 1) as u64,
        4,
    );
    cases.push((
        "the last section's length past the body".to_string(),
        long,
        ProtocolErrorEnum::SectionOutOfBounds {
            command,
            section: n - 1,
            offset: offset as u32,
            len: (data.len() - offset + 1) as u32,
            body: data.len(),
        },
    ));
    let _ = len;
    // Misaligned: section 0 one byte later, its length one shorter so it stays inside.
    let (offset0, len0) = section(&data, 0, 0);
    if len0 > 0 {
        let mut misaligned = data.clone();
        put(&mut misaligned, 0, (offset0 + 1) as u64, 4);
        put(&mut misaligned, 4, (len0 - 1) as u64, 4);
        cases.push((
            "a misaligned section".to_string(),
            misaligned,
            ProtocolErrorEnum::SectionOffset {
                command,
                section: 0,
                offset: (offset0 + 1) as u32,
                expected: 8 * n,
            },
        ));
    }
    // Overlapping and out of order: section 1 placed at section 0's offset; sections 0 and 1 swapped.
    if n >= 2 && len0 > 0 {
        let (offset1, len1) = section(&data, 0, 1);
        let mut overlapping = data.clone();
        put(&mut overlapping, 8, offset0 as u64, 4);
        put(&mut overlapping, 12, len1.min(len0) as u64, 4);
        let expected1 = (offset0 + len0).div_ceil(8) * 8;
        cases.push((
            "overlapping sections".to_string(),
            overlapping,
            ProtocolErrorEnum::SectionOffset {
                command,
                section: 1,
                offset: offset0 as u32,
                expected: expected1,
            },
        ));
        if len1 > 0 {
            let mut swapped = data.clone();
            put(&mut swapped, 0, offset1 as u64, 4);
            put(&mut swapped, 4, len1 as u64, 4);
            put(&mut swapped, 8, offset0 as u64, 4);
            put(&mut swapped, 12, len0 as u64, 4);
            cases.push((
                "sections out of order".to_string(),
                swapped,
                ProtocolErrorEnum::SectionOffset {
                    command,
                    section: 0,
                    offset: offset1 as u32,
                    expected: 8 * n,
                },
            ));
        }
    }
    // A gap: eight zero bytes before the last section, its offset moved past them.
    let (last_offset, _) = section(&data, 0, n - 1);
    let mut gapped = data[..last_offset].to_vec();
    gapped.extend_from_slice(&[0; 8]);
    gapped.extend_from_slice(&data[last_offset..]);
    put(&mut gapped, 8 * (n - 1), (last_offset + 8) as u64, 4);
    cases.push((
        "a gap before the last section".to_string(),
        gapped,
        ProtocolErrorEnum::SectionOffset {
            command,
            section: n - 1,
            offset: (last_offset + 8) as u32,
            expected: last_offset,
        },
    ));
    // Padding: the first padding byte between two sections set.
    let padding = (1..n).find_map(|i| {
        let (o, l) = section(&data, 0, i - 1);
        let next = section(&data, 0, i).0;
        (o + l < next).then_some(o + l)
    });
    if let Some(at) = padding {
        let mut padded = data.clone();
        padded[at] = 1;
        cases.push((
            "nonzero padding".to_string(),
            padded,
            ProtocolErrorEnum::PaddingNotZero {
                command,
                offset: at,
            },
        ));
    }
    cases
}

#[test]
fn test_each_layout_rule_is_refused_naming_its_section() {
    let mut seen = std::collections::BTreeSet::new();
    for body in BODIES {
        let bytes = body.bytes();
        for (case, data, want) in layout_cases(body) {
            let frame = reframe(&bytes, &data);
            assert_eq!(refusal(&frame), want, "{}: {case}", body.name);
            seen.insert(case);
        }
    }
    // Every case met at least one body.
    assert_eq!(seen.len(), 9, "{seen:?}");
}

#[test]
fn test_a_view_of_another_command_is_refused() {
    let hello = golden("hello").bytes();
    let welcome = golden("welcome").bytes();
    let frame = FrameView::validate(&welcome, CLUSTER).unwrap();
    assert_eq!(
        HelloView::from_frame(&frame).err(),
        Some(ProtocolError(ProtocolErrorEnum::WrongCommand {
            expected: 1,
            found: 2
        }))
    );
    let frame = FrameView::validate(&hello, CLUSTER).unwrap();
    assert_eq!(
        WelcomeView::from_frame(&frame).unwrap_err().0,
        ProtocolErrorEnum::WrongCommand {
            expected: 2,
            found: 1
        }
    );
    assert_eq!(
        CreditView::from_frame(&frame).unwrap_err().0,
        ProtocolErrorEnum::WrongCommand {
            expected: 14,
            found: 1
        }
    );
    assert_eq!(
        RoundView::from_frame(&frame).unwrap_err().0,
        ProtocolErrorEnum::WrongCommand {
            expected: 17,
            found: 1
        }
    );
}

#[test]
fn test_a_frame_relabelled_as_every_other_command_never_panics() {
    // Each golden body under every other command number: its view reads it as that command's body and must refuse
    // it or read it, never panic.
    for body in BODIES {
        let bytes = body.bytes();
        for command in 1..=17u16 {
            let _ = view_any(&with_command(&bytes, command));
        }
    }
    // A body of no sections relabelled as Read, whose zero command header is valid, is too short for its table.
    let abort = golden("abort").bytes();
    assert_eq!(
        refusal(&with_command(&abort, 7)),
        ProtocolErrorEnum::BodyTooShort {
            command: 7,
            len: 0,
            need: 32
        }
    );
    // The command header is checked before the body: Credit's frames read as Hello's protocol range.
    let credit = golden("credit").bytes();
    assert_eq!(
        refusal(&with_command(&credit, 1)),
        ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "protocol_max",
            value: 0
        }
    );
    assert_eq!(u32_at(&credit[128..], 0), 2);
}
