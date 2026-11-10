/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Describe answer across continuation frames (`PROTOCOL.md` §8.3, *Continuation*): built by `Describe::frames`
//! within a `max_frame`, read back by `DescribeFrames`, against the answer laid out by hand.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyView, CATALOG_BYTES_MAX, Command, Describe, DescribeEntry, DescribeFrames, EntryKind,
    FLAG_LAST, Header, MAX_FRAME_SIZE, RetiredItem, RetiredName,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{CLUSTER, body_data, frame_of, split_sections};
use crate::utils::ugs_frames::{describe_frame, entry_sections, frame, u32s};

/// A source of 9,000 bytes of ASCII, which three frames of 4,096 bytes carry.
fn long_source() -> String {
    "namespace gen5 { }\n".repeat(474)
}

fn answer() -> Describe {
    Describe::new(9, 42).with_entries(vec![DescribeEntry::new(
        EntryKind::Source,
        "gen5",
        long_source().into_bytes(),
    )])
}

fn header() -> Header {
    Header::new(CLUSTER, Command::Describe).with_stream(3)
}

/// The frames of `frames` taken in order.
fn take(frames: &[Vec<u8>]) -> TestResult<DescribeFrames> {
    let mut taken = DescribeFrames::new();
    for bytes in frames {
        taken.push(&frame_of(bytes)?)?;
    }
    Ok(taken)
}

/// wire-protocol, "A Describe past the client's frame continues": frames within `max_frame`, each with the first's
/// command header, whose sections concatenate to the whole answer, the last alone flagged `LAST`.
#[test]
fn test_an_answer_past_max_frame_continues_and_concatenates() -> TestResult {
    let frames = answer().frames(header(), 4_096)?;
    assert!(frames.len() >= 3, "{} frames", frames.len());
    let (last, before) = frames.split_last().context("frames")?;
    for bytes in &frames {
        assert!(bytes.len() <= 4_096, "a frame of {} bytes", bytes.len());
        let view = frame_of(bytes)?;
        assert_eq!(view.command(), Command::Describe);
        assert_eq!(
            view.header().command_header(),
            frame_of(last)?.header().command_header()
        );
    }
    for bytes in before {
        assert_eq!(frame_of(bytes)?.header().flags() & FLAG_LAST, 0);
    }
    assert_eq!(frame_of(last)?.header().flags() & FLAG_LAST, FLAG_LAST);
    // Each section the concatenation of that section in every frame: the whole answer laid out by hand.
    let whole = describe_frame(
        &["gen5"],
        entry_sections(&[(19, 0, long_source().as_bytes())]),
        true,
    )?;
    let expected = split_sections(body_data(&whole), 0, 9);
    let mut joined = vec![Vec::new(); 9];
    for bytes in &frames {
        for (into, part) in joined
            .iter_mut()
            .zip(split_sections(body_data(bytes), 0, 9))
        {
            into.extend_from_slice(&part);
        }
    }
    assert_eq!(joined, expected);
    let taken = take(&frames)?;
    assert!(taken.is_last());
    let view = taken.view()?;
    let entry = view.entry(0).context("the source")?;
    assert_eq!(entry.bytes(), long_source().as_bytes());
    Ok(())
}

/// An answer that fits is one frame flagged `LAST`, the answer laid out by hand.
#[test]
fn test_an_answer_that_fits_is_one_frame() -> TestResult {
    let small = Describe::new(9, 42).with_entries(vec![DescribeEntry::new(
        EntryKind::Source,
        "gen5",
        b"namespace gen5 {}".to_vec(),
    )]);
    let frames = small.frames(header(), MAX_FRAME_SIZE)?;
    let expected = describe_frame(
        &["gen5"],
        entry_sections(&[(19, 0, b"namespace gen5 {}")]),
        true,
    )?;
    assert_eq!(frames, vec![expected]);
    Ok(())
}

/// A frame whose command header is not the first's is refused as `FieldValue` naming `describe`; a frame of another
/// command as the wrong command.
#[test]
fn test_a_continuation_keeps_the_first_command_header() -> TestResult {
    let first = describe_frame(&["a"], entry_sections(&[]), false)?;
    let other = frame(
        Command::Describe,
        &[(0, &9u64.to_le_bytes()), (8, &43u64.to_le_bytes())],
        &vec![vec![]; 9],
        true,
    )?;
    let mut taken = DescribeFrames::new();
    taken.push(&frame_of(&first)?)?;
    assert!(!taken.is_last());
    let refusal = taken.push(&frame_of(&other)?).err().context("a refusal")?.0;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FieldValue {
                command: 12,
                field: "describe",
                ..
            }
        ),
        "{refusal:?}"
    );
    let catalog = frame(Command::Catalog, &[], &[vec![]], true)?;
    assert_eq!(
        DescribeFrames::new()
            .push(&frame_of(&catalog)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::WrongCommand {
            expected: 12,
            found: 518
        }
    );
    Ok(())
}

/// A frame splits a section only between whole u32s or records and its text only on a character boundary: each
/// frame's u32 sections whole u32s, its records whole 16-byte records, its strings' text UTF-8.
#[test]
fn test_each_frame_holds_whole_units() -> TestResult {
    let part = |index: usize, bytes: Vec<u8>| -> TestResult<Vec<u8>> {
        let mut sections = vec![vec![]; 9];
        if let Some(section) = sections.get_mut(index) {
            *section = bytes;
        }
        frame(
            Command::Describe,
            &[(8, &42u64.to_le_bytes())],
            &sections,
            false,
        )
    };
    let cases = [
        (
            2,
            vec![0, 0, 0],
            ProtocolErrorEnum::SectionLength {
                command: 12,
                section: 2,
                len: 3,
                unit: 4,
            },
        ),
        (
            6,
            vec![0; 5],
            ProtocolErrorEnum::SectionLength {
                command: 12,
                section: 6,
                len: 5,
                unit: 4,
            },
        ),
        (
            5,
            vec![0; 8],
            ProtocolErrorEnum::SectionLength {
                command: 12,
                section: 5,
                len: 8,
                unit: 16,
            },
        ),
        (
            7,
            vec![0; 8],
            ProtocolErrorEnum::SectionLength {
                command: 12,
                section: 7,
                len: 8,
                unit: 16,
            },
        ),
        (
            1,
            vec![b'a', 0xC3],
            ProtocolErrorEnum::NotUtf8 {
                command: 12,
                section: 1,
                offset: 1,
            },
        ),
    ];
    for (index, bytes, expected) in cases {
        let refusal = DescribeFrames::new()
            .push(&frame_of(&part(index, bytes)?)?)
            .err()
            .context("a refusal")?
            .0;
        assert_eq!(refusal, expected);
    }
    // A u32 section of whole u32s, split anywhere between them, is taken.
    let mut taken = DescribeFrames::new();
    taken.push(&frame_of(&part(0, u32s(&[1]))?)?)?;
    Ok(())
}

/// Before the first frame the answer is no body: too short for nine sections.
#[test]
fn test_no_frame_reads_as_no_body() -> TestResult {
    assert_eq!(
        DescribeFrames::new().view().err().context("a refusal")?.0,
        ProtocolErrorEnum::BodyTooShort {
            command: 12,
            len: 0,
            need: 72
        }
    );
    Ok(())
}

/// A frame of an answer of tenant 9 read at 42 holding `len` bytes in section 8, every other section empty, not flagged
/// `LAST`.
fn bytes_frame(len: usize) -> TestResult<Vec<u8>> {
    let mut sections = vec![vec![]; 9];
    if let Some(section) = sections.get_mut(8) {
        *section = vec![0; len];
    }
    frame(
        Command::Describe,
        &[(0, &9u64.to_le_bytes()), (8, &42u64.to_le_bytes())],
        &sections,
        false,
    )
}

/// PROTOCOL.md §8.3, *Continuation*: an answer's sections hold at most `catalog_bytes_max` bytes together, and a client
/// refuses a stream passing it as `FieldValue` naming `describe`. Frames of 1,000,000 bytes each and one of the rest
/// reach the bound exactly and are taken; one byte more is refused.
#[test]
fn test_an_answer_past_catalog_bytes_max_is_refused() -> TestResult {
    const FRAME_BYTES: usize = 1_000_000;
    let mut taken = DescribeFrames::new();
    for _ in 0..CATALOG_BYTES_MAX / FRAME_BYTES {
        taken.push(&frame_of(&bytes_frame(FRAME_BYTES)?)?)?;
    }
    taken.push(&frame_of(&bytes_frame(CATALOG_BYTES_MAX % FRAME_BYTES)?)?)?;
    let refusal = taken
        .push(&frame_of(&bytes_frame(1)?)?)
        .err()
        .context("a refusal")?
        .0;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FieldValue {
                command: 12,
                field: "describe",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// Section 8's part in each of `frames`, in frame order.
fn section_8_parts(frames: &[Vec<u8>]) -> Vec<Vec<u8>> {
    frames
        .iter()
        .map(|bytes| {
            split_sections(body_data(bytes), 0, 9)
                .into_iter()
                .nth(8)
                .unwrap_or_default()
        })
        .collect()
}

/// A source's text is cut only on a character boundary (§8.3, *Continuation*), each frame else holding as much as fits
/// within `max_frame`, its body one chunk of at most 3,840 bytes within 4,096 (§2.3, §4.1). The first frame holds the
/// table's 72 bytes, the name's end (to 76), the name (from 80, to 84) and the entry (from 88, to 104), leaving 3,736
/// bytes for the source, cut back to 3,735, the last boundary of its 3-byte characters at or below it; each frame after
/// it the table and 3,768 bytes, a whole number of characters; the last the 1,497 left of 9,000.
#[test]
fn test_a_sources_text_is_cut_on_a_character_boundary_and_fills_each_frame() -> TestResult {
    let answer = Describe::new(9, 42).with_entries(vec![DescribeEntry::new(
        EntryKind::Source,
        "gen5",
        "€".repeat(3_000).into_bytes(),
    )]);
    let frames = answer.frames(header(), 4_096)?;
    let parts = section_8_parts(&frames);
    assert_eq!(
        parts.iter().map(Vec::len).collect::<Vec<_>>(),
        [3_735, 3_768, 1_497]
    );
    for part in &parts {
        assert!(
            std::str::from_utf8(part).is_ok(),
            "a part of {} bytes cut inside a character",
            part.len()
        );
    }
    for bytes in &frames {
        assert!(bytes.len() <= 4_096, "a frame of {} bytes", bytes.len());
    }
    assert_eq!(take(&frames)?.view()?.to_body()?, answer);
    Ok(())
}

/// A cut outside a source's text falls where the frame's room ends, a retired name's bytes being no text (§8.3): a source
/// of 3,500 bytes of ASCII, then ten retired names whose versions and windows are bytes 0xBF, the bytes that continue a
/// UTF-8 character. Within 4,096 the first frame holds the table's 72 bytes, the eleven names' ends (44 bytes, to 116),
/// their 24 bytes of text (from 120, to 144), the eleven entries (from 144, to 320) and 3,520 bytes of section 8, which
/// cut the first retired name at its byte 20; the second frame the 220 left.
#[test]
fn test_a_cut_outside_a_text_falls_where_the_room_ends() -> TestResult {
    let continuing = 0xBFBF_BFBF_BFBF_BFBF;
    let mut entries = vec![DescribeEntry::new(
        EntryKind::Source,
        "gen5",
        vec![b'a'; 3_500],
    )];
    entries.extend((0..10).map(|index| {
        DescribeEntry::retired(
            format!("r{index}"),
            RetiredName::renamed(RetiredItem::Type, "gen5", continuing).until(continuing),
        )
    }));
    let answer = Describe::new(9, 42).with_entries(entries);
    let frames = answer.frames(header(), 4_096)?;
    assert_eq!(
        section_8_parts(&frames)
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [3_520, 220]
    );
    assert_eq!(take(&frames)?.view()?.to_body()?, answer);
    Ok(())
}

/// A `max_frame` too small for a frame carrying any part of the answer is refused as `FrameTooLarge`, naming the limit
/// and a frame larger than it: within 300 bytes the header leaves 44 for a body whose section table alone takes 72.
#[test]
fn test_a_max_frame_holding_no_part_is_refused() -> TestResult {
    let refusal = answer().frames(header(), 300).err().context("a refusal")?.0;
    assert!(
        matches!(refusal, ProtocolErrorEnum::FrameTooLarge { size, limit: 300 } if size > 300),
        "{refusal:?}"
    );
    Ok(())
}


/// A `max_frame` carrying no part of the answer is refused as `FrameTooLarge` naming the least frame that holds the next
/// unit of a section with bytes left, the least over those sections (§2.3, §4.1): the frame header and command header,
/// 256 bytes, the table of nine sections, 72, and the unit at its section's start, padded to 8 for the empty sections
/// after it unless its section is the last.
#[test]
fn test_a_max_frame_below_the_least_frame_names_it() -> TestResult {
    let answer_of = |kind, name: &str, bytes: &[u8]| {
        Describe::new(9, 42).with_entries(vec![DescribeEntry::new(kind, name, bytes.to_vec())])
    };
    let cases = [
        // The source's first byte, in section 8, the last: 256 + 72 + 1 = 329 (a name's end takes 256 + 72 + 8 = 336).
        (answer(), 329),
        // No bytes in section 8: a name's end, 4 bytes, or its first character, padded to 8: 256 + 72 + 8 = 336 (the
        // entry's 16 bytes take 256 + 72 + 16 = 344).
        (answer_of(EntryKind::Type, "T", b""), 336),
        // A source beginning with a character of 3 bytes, `€`: 256 + 72 + 3 = 331.
        (answer_of(EntryKind::Source, "s", "€x".as_bytes()), 331),
        // A type's bytes, no text, whose bytes after the first continue a UTF-8 character: one byte, 256 + 72 + 1 = 329.
        (answer_of(EntryKind::Type, "T", &[0x41, 0x80, 0x80]), 329),
    ];
    for (answer, least) in cases {
        let refusal = answer
            .frames(header(), least - 1)
            .err()
            .context("a refusal")?
            .0;
        assert_eq!(
            refusal,
            ProtocolErrorEnum::FrameTooLarge {
                size: least,
                limit: least - 1
            }
        );
    }
    Ok(())
}

/// A section before the last ends where the frame's room, rounded down to 8, ends, so the padding before the sections
/// after it fits (§4.1): 300 types, `t000` to `t299`, of no bytes. Within 4,096 a body is one chunk of at most 3,840
/// bytes: the table (to 72), the names' ends (1,200 bytes, to 1,272), their text (1,200, to 2,472) and 85 entries (1,360,
/// to 3,832; an 86th would end at 3,848); the second frame the other 215 entries (3,440 bytes).
#[test]
fn test_a_section_before_the_last_is_cut_where_the_room_ends() -> TestResult {
    let answer = Describe::new(9, 42).with_entries(
        (0..300)
            .map(|index| DescribeEntry::new(EntryKind::Type, format!("t{index:03}"), Vec::new()))
            .collect(),
    );
    let frames = answer.frames(header(), 4_096)?;
    let parts: Vec<Vec<usize>> = frames
        .iter()
        .map(|bytes| {
            split_sections(body_data(bytes), 0, 9)
                .iter()
                .map(Vec::len)
                .collect()
        })
        .collect();
    assert_eq!(
        parts,
        [
            [1_200, 1_200, 0, 0, 0, 0, 0, 1_360, 0],
            [0, 0, 0, 0, 0, 0, 0, 3_440, 0]
        ]
    );
    for bytes in &frames {
        assert!(bytes.len() <= 4_096, "a frame of {} bytes", bytes.len());
    }
    assert_eq!(take(&frames)?.view()?.to_body()?, answer);
    Ok(())
}
