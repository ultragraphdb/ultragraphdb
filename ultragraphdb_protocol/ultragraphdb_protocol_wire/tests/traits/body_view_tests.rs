/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conformance suite's malformed frames, over every golden body: a case per rule of the body layout, every
//! truncation, every bit flip of the body and of the command header, unused command-header bytes, and a view asked of
//! another command's frame. Each is refused with its `ProtocolError`, and none panics.

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyView, CreditView, FrameView, HelloView, MAX_FRAME_SIZE, RoundView, WelcomeView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{
    BODIES, CLUSTER, GoldenBody, body_data, golden, put, reframe, section, u32_at, view_any,
    with_command, with_command_header,
};

/// The sections each command's body has, as its layout table states.
fn sections_of(command: u16) -> usize {
    match command {
        1 => 1,
        2 => 3,
        3 | 4 | 6 | 14 | 15 => 0,
        5 => 3,
        7 => 4,
        8 => 6,
        9 => 3,
        10 => 10,
        11 => 4,
        12 => 9,
        13 => 12,
        16 => 3,
        17 => 4,
        18 => 2,
        19 => 2,
        20 => 6,
        21 | 22 => 7,
        23 => 7,
        _ => unreachable!("no such command"),
    }
}

/// The command-header bytes each command's fields use, as its layout table states.
fn used(command: u16) -> Vec<(usize, usize)> {
    match command {
        1 => vec![(0, 12)],
        2 => vec![(0, 3), (4, 28)],
        3 => vec![(0, 16)],
        4 => vec![(0, 18), (20, 48)],
        5 => vec![(0, 56)],
        6 | 8 | 15 => vec![],
        7 => vec![(0, 16)],
        9 => vec![(0, 1), (8, 24)],
        10 => vec![(0, 25)],
        11 => vec![(0, 24)],
        12 => vec![(0, 16)],
        13 => vec![(0, 21)],
        14 => vec![(0, 4)],
        16 => vec![(0, 2), (8, 16)],
        17 => vec![(0, 40)],
        18 => vec![(0, 5), (8, 24)],
        19 => vec![(0, 1), (8, 80)],
        20 | 21 => vec![(0, 64)],
        22 => vec![(0, 9), (12, 16)],
        23 => vec![(0, 32)],
        _ => unreachable!("no such command"),
    }
}

fn refusal(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(view_any(bytes)?.err().context("the frame is refused")?.0)
}

#[test]
fn test_every_golden_body_reads_and_rebuilds() -> TestResult {
    for body in BODIES {
        let bytes = body.bytes()?;
        assert_eq!(bytes.len(), body.size as usize, "{}", body.name);
        let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE)
            .context("a golden frame validates")?;
        assert_eq!(frame.command().number(), body.command, "{}", body.name);
        assert_eq!(frame.header().checksum(), body.checksum, "{}", body.name);
        assert_eq!(
            frame.header().checksum_body(),
            body.checksum_body,
            "{}",
            body.name
        );
        assert_eq!(view_any(&bytes)?, Ok(()), "{}", body.name);
    }
    Ok(())
}

#[test]
fn test_every_truncation_of_every_body_is_refused() -> TestResult {
    // PROTOCOL.md section 4.1: a body shorter than its table is `BodyTooShort`; otherwise the first section, in table
    // order, running past the shortened body is `SectionOutOfBounds`. The expectation is read off the golden body's
    // own table by the test's helpers, not by the crate.
    for body in BODIES {
        let bytes = body.bytes()?;
        let data = body_data(&bytes);
        let n = sections_of(body.command);
        for len in 0..data.len() {
            let truncated = reframe(&bytes, &data[..len]);
            let want = if len < 8 * n {
                ProtocolErrorEnum::BodyTooShort {
                    command: body.command,
                    len,
                    need: 8 * n,
                }
            } else {
                let (section, (offset, slen)) = (0..n)
                    .map(|i| (i, section(data, 0, i)))
                    .find(|(_, (offset, slen))| offset + slen > len)
                    .context("a truncation cuts a section")?;
                ProtocolErrorEnum::SectionOutOfBounds {
                    command: body.command,
                    section,
                    offset: u32::try_from(offset)?,
                    len: u32::try_from(slen)?,
                    body: len,
                }
            };
            assert_eq!(refusal(&truncated)?, want, "{} at {len}", body.name);
        }
    }
    Ok(())
}

#[test]
fn test_every_bit_flip_of_every_body_reads_or_is_refused_without_a_panic() -> TestResult {
    for body in BODIES {
        let bytes = body.bytes()?;
        let data = body_data(&bytes).to_vec();
        for bit in 0..data.len() * 8 {
            let mut flipped = data.clone();
            flipped[bit / 8] ^= 1 << (bit % 8);
            // A flip may give another valid body, a term's text changed; it must never panic.
            let _read_or_refused: Result<(), ProtocolError> = view_any(&reframe(&bytes, &flipped))?;
        }
    }
    Ok(())
}

#[test]
fn test_every_bit_flip_of_every_command_header_reads_or_is_refused_without_a_panic() -> TestResult {
    for body in BODIES {
        let bytes = body.bytes()?;
        for bit in 0..128 * 8 {
            let flipped = with_command_header(&bytes, |h| h[bit / 8] ^= 1 << (bit % 8));
            let _read_or_refused: Result<(), ProtocolError> = view_any(&flipped)?;
        }
    }
    Ok(())
}

#[test]
fn test_each_unused_command_header_byte_must_be_zero() -> TestResult {
    for body in BODIES {
        let bytes = body.bytes()?;
        let used = used(body.command);
        for at in (0..128).filter(|at| !used.iter().any(|(start, end)| (*start..*end).contains(at)))
        {
            let changed = with_command_header(&bytes, |h| h[at] = 0x40);
            assert_eq!(
                refusal(&changed)?,
                ProtocolErrorEnum::ReservedNotZero { offset: 128 + at },
                "{} at {at}",
                body.name
            );
        }
    }
    Ok(())
}

/// A layout case: its name, the body data, and the refusal it gets.
type LayoutCase = (String, Vec<u8>, ProtocolErrorEnum);

fn layout_cases(body: GoldenBody) -> TestResult<Vec<LayoutCase>> {
    let bytes = body.bytes()?;
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
        return Ok(cases);
    }
    bounds_cases(command, n, &data, &mut cases)?;
    order_cases(command, n, &data, &mut cases)?;
    gap_cases(command, n, &data, &mut cases)?;
    Ok(cases)
}

/// The cases of a table or a section outside the body of `data`, `command`'s, of `n` sections.
fn bounds_cases(command: u16, n: usize, data: &[u8], cases: &mut Vec<LayoutCase>) -> TestResult {
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
    let mut past = data.to_vec();
    let past_at = u32::try_from(data.len() + 8)?;
    put(&mut past, 0, past_at.into(), 4);
    cases.push((
        "an offset past the body".to_string(),
        past,
        ProtocolErrorEnum::SectionOutOfBounds {
            command,
            section: 0,
            offset: past_at,
            len: u32::try_from(section(data, 0, 0).1)?,
            body: data.len(),
        },
    ));
    // A length past the body.
    let mut long = data.to_vec();
    let (offset, _) = section(data, 0, n - 1);
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
            offset: u32::try_from(offset)?,
            len: u32::try_from(data.len() - offset + 1)?,
            body: data.len(),
        },
    ));
    Ok(())
}

/// The cases of sections misaligned, overlapping or out of order in `data`, `command`'s, of `n` sections.
fn order_cases(command: u16, n: usize, data: &[u8], cases: &mut Vec<LayoutCase>) -> TestResult {
    // Misaligned: section 0 one byte later, its length one shorter so it stays inside.
    let (offset0, len0) = section(data, 0, 0);
    if len0 > 0 {
        let mut misaligned = data.to_vec();
        put(&mut misaligned, 0, (offset0 + 1) as u64, 4);
        put(&mut misaligned, 4, (len0 - 1) as u64, 4);
        cases.push((
            "a misaligned section".to_string(),
            misaligned,
            ProtocolErrorEnum::SectionOffset {
                command,
                section: 0,
                offset: u32::try_from(offset0 + 1)?,
                expected: 8 * n,
            },
        ));
    }
    // Overlapping and out of order: section 1 placed at section 0's offset; sections 0 and 1 swapped.
    if n >= 2 && len0 > 0 {
        let (offset1, len1) = section(data, 0, 1);
        let mut overlapping = data.to_vec();
        put(&mut overlapping, 8, offset0 as u64, 4);
        put(&mut overlapping, 12, len1.min(len0) as u64, 4);
        let expected1 = (offset0 + len0).div_ceil(8) * 8;
        cases.push((
            "overlapping sections".to_string(),
            overlapping,
            ProtocolErrorEnum::SectionOffset {
                command,
                section: 1,
                offset: u32::try_from(offset0)?,
                expected: expected1,
            },
        ));
        if len1 > 0 {
            let mut swapped = data.to_vec();
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
                    offset: u32::try_from(offset1)?,
                    expected: 8 * n,
                },
            ));
        }
    }
    Ok(())
}

/// The cases of a gap or nonzero padding between sections of `data`, `command`'s, of `n` sections.
fn gap_cases(command: u16, n: usize, data: &[u8], cases: &mut Vec<LayoutCase>) -> TestResult {
    // A gap: eight zero bytes before the last section, its offset moved past them.
    let (last_offset, _) = section(data, 0, n - 1);
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
            offset: u32::try_from(last_offset + 8)?,
            expected: last_offset,
        },
    ));
    // Padding: the first padding byte between two sections set.
    let padding = (1..n).find_map(|i| {
        let (o, l) = section(data, 0, i - 1);
        let next = section(data, 0, i).0;
        (o + l < next).then_some(o + l)
    });
    if let Some(at) = padding {
        let mut padded = data.to_vec();
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
    Ok(())
}

#[test]
fn test_each_layout_rule_is_refused_naming_its_section() -> TestResult {
    let mut seen = std::collections::BTreeSet::new();
    for body in BODIES {
        let bytes = body.bytes()?;
        for (case, data, want) in layout_cases(body)? {
            let frame = reframe(&bytes, &data);
            assert_eq!(refusal(&frame)?, want, "{}: {case}", body.name);
            let _first_seen: bool = seen.insert(case);
        }
    }
    // Every case met at least one body.
    assert_eq!(seen.len(), 9, "{seen:?}");
    Ok(())
}

#[test]
fn test_a_view_of_another_command_is_refused() -> TestResult {
    let hello = golden("hello")?.bytes()?;
    let welcome = golden("welcome")?.bytes()?;
    let frame = FrameView::validate(&welcome, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(
        HelloView::from_frame(&frame).err(),
        Some(ProtocolError(ProtocolErrorEnum::WrongCommand {
            expected: 1,
            found: 2
        }))
    );
    let hello_frame = FrameView::validate(&hello, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(
        WelcomeView::from_frame(&hello_frame)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::WrongCommand {
            expected: 2,
            found: 1
        }
    );
    assert_eq!(
        CreditView::from_frame(&hello_frame)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::WrongCommand {
            expected: 14,
            found: 1
        }
    );
    assert_eq!(
        RoundView::from_frame(&hello_frame)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::WrongCommand {
            expected: 17,
            found: 1
        }
    );
    Ok(())
}

#[test]
fn test_a_frame_relabelled_as_every_other_command_never_panics() -> TestResult {
    // Each golden body under every other command number: its view reads it as that command's body and must refuse
    // it or read it, never panic.
    for body in BODIES {
        let bytes = body.bytes()?;
        for command in 1..=21u16 {
            let _read_or_refused: Result<(), ProtocolError> =
                view_any(&with_command(&bytes, command))?;
        }
    }
    // A body of no sections relabelled as Read, whose zero command header is valid, is too short for its table.
    let abort = golden("abort")?.bytes()?;
    assert_eq!(
        refusal(&with_command(&abort, 7))?,
        ProtocolErrorEnum::BodyTooShort {
            command: 7,
            len: 0,
            need: 32
        }
    );
    // The command header is checked before the body: Credit's frames read as Hello's protocol range.
    let credit = golden("credit")?.bytes()?;
    assert_eq!(
        refusal(&with_command(&credit, 1))?,
        ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "protocol_max",
            value: 0
        }
    );
    assert_eq!(u32_at(&credit[128..], 0), 2);
    Ok(())
}
