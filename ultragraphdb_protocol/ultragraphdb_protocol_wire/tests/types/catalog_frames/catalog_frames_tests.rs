/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A catalog version across continuation Catalog frames (`PROTOCOL.md` §13.7): built by `Catalog::frames` within a
//! `max_frame`, read back by `CatalogFrames`, against frames laid out by hand.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    CATALOG_BYTES_MAX, Catalog, CatalogFrames, Command, FLAG_LAST, Header, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{CLUSTER, body_data, frame_of, split_sections};
use crate::utils::ugs_frames::frame;

/// 10,000 bytes, every byte its index's low byte, which three frames of 4,096 carry.
fn version_bytes() -> Vec<u8> {
    (0..10_000u32).map(|i| i.to_le_bytes()[0]).collect()
}

fn header() -> Header {
    Header::new(CLUSTER, Command::Catalog).with_stream(3)
}

/// The reply's command header fields: tenant 1, version 7, epoch 30, position 2.
fn fields() -> [(usize, Vec<u8>); 4] {
    [
        (0, 1u64.to_le_bytes().to_vec()),
        (8, 7u64.to_le_bytes().to_vec()),
        (16, 30u64.to_le_bytes().to_vec()),
        (24, 2u64.to_le_bytes().to_vec()),
    ]
}

/// A version past one frame continues in Catalog frames within `max_frame`, each with the first's command header,
/// the last alone flagged `LAST`, section 0 their concatenation.
#[test]
fn test_a_version_past_max_frame_continues() -> TestResult {
    let reply = Catalog::reply(1, 7, 30, 2, version_bytes());
    let frames = reply.frames(header(), 4_096)?;
    assert!(frames.len() >= 3, "{} frames", frames.len());
    let mut joined = Vec::new();
    let mut taken = CatalogFrames::new();
    for (index, bytes) in frames.iter().enumerate() {
        assert!(bytes.len() <= 4_096);
        let view = frame_of(bytes)?;
        let last = index + 1 == frames.len();
        assert_eq!(view.header().flags() & FLAG_LAST == FLAG_LAST, last);
        joined.extend(split_sections(body_data(bytes), 0, 1).concat());
        taken.push(&view)?;
    }
    assert_eq!(joined, version_bytes());
    assert!(taken.is_last());
    let whole = taken.view().context("the version")?;
    assert_eq!(
        (
            whole.tenant(),
            whole.version(),
            whole.epoch(),
            whole.position()
        ),
        (1, 7, 30, 2)
    );
    assert_eq!(whole.bytes(), version_bytes().as_slice());
    Ok(())
}

/// A version that fits is one frame flagged `LAST`, laid out by hand.
#[test]
fn test_a_version_that_fits_is_one_frame() -> TestResult {
    let reply = Catalog::reply(1, 7, 30, 2, b"UGCV".to_vec());
    let fields = fields();
    let refs: Vec<(usize, &[u8])> = fields.iter().map(|(at, b)| (*at, b.as_slice())).collect();
    let expected = frame(Command::Catalog, &refs, &[b"UGCV".to_vec()], true)?;
    assert_eq!(reply.frames(header(), MAX_FRAME_SIZE)?, vec![expected]);
    Ok(())
}

/// A frame breaking the first's command header is refused as `FieldValue` naming `catalog`; nothing is read before
/// the first frame.
#[test]
fn test_a_continuation_keeps_the_first_command_header() -> TestResult {
    assert!(CatalogFrames::new().view().is_none());
    let fields = fields();
    let refs: Vec<(usize, &[u8])> = fields.iter().map(|(at, b)| (*at, b.as_slice())).collect();
    let first = frame(Command::Catalog, &refs, &[b"UG".to_vec()], false)?;
    let mut other_fields = refs.clone();
    other_fields[1] = (8, &[8, 0, 0, 0, 0, 0, 0, 0]);
    let other = frame(Command::Catalog, &other_fields, &[b"CV".to_vec()], true)?;
    let mut taken = CatalogFrames::new();
    taken.push(&frame_of(&first)?)?;
    assert!(!taken.is_last());
    let refusal = taken.push(&frame_of(&other)?).err().context("a refusal")?.0;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FieldValue {
                command: 518,
                field: "catalog",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// A reply frame of the fields above holding `len` bytes of the version, not flagged `LAST`.
fn bytes_frame(len: usize) -> TestResult<Vec<u8>> {
    let fields = fields();
    let refs: Vec<(usize, &[u8])> = fields.iter().map(|(at, b)| (*at, b.as_slice())).collect();
    frame(Command::Catalog, &refs, &[vec![0; len]], false)
}

/// PROTOCOL.md §13.7: a requester refuses a stream whose section 0 passes `catalog_bytes_max` as `FieldValue` naming
/// `catalog`. Frames of 1,000,000 bytes each and one of the rest reach the bound exactly and are taken; one byte more is
/// refused.
#[test]
fn test_a_version_past_catalog_bytes_max_is_refused() -> TestResult {
    const FRAME_BYTES: usize = 1_000_000;
    let mut taken = CatalogFrames::new();
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
                command: 518,
                field: "catalog",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// Each frame but the last holds as many of the version's bytes as fit within `max_frame` (§13.7: the bytes that do not
/// fit one frame continue in the next), the frame laid out by §2.3 and §4.1: the 256-byte header, the section table's 8
/// bytes, the bytes, and a table of 8 bytes a chunk once the body passes one chunk of 3,840 bytes, every chunk after the
/// first of 4,096. Within 2,000 one chunk: 2,000 − 256 − 8 = 1,736 bytes. Within 16,384 the 16,128 bytes after the
/// header take four chunks, at most 3,840 + 3 × 4,096 = 16,128 bytes beside a table of 32, so a body of 16,096 and
/// 16,088 of the version's bytes (three chunks hold 12,032, five leave 16,088 for the body). Within 16,456 five chunks,
/// a body of 16,200 − 40 = 16,160, 16,152 of the bytes (four hold 16,128).
#[test]
fn test_each_frame_but_the_last_holds_what_fits() -> TestResult {
    let cases: [(usize, u32, [usize; 3]); 3] = [
        (2_000, 4_000, [1_736, 1_736, 528]),
        (16_384, 40_000, [16_088, 16_088, 7_824]),
        (16_456, 40_000, [16_152, 16_152, 7_696]),
    ];
    for (max_frame, len, expected) in cases {
        let bytes: Vec<u8> = (0..len).map(|i| i.to_le_bytes()[0]).collect();
        let frames = Catalog::reply(1, 7, 30, 2, bytes.clone()).frames(header(), max_frame)?;
        let mut held = Vec::new();
        let mut joined = Vec::new();
        for frame in &frames {
            assert!(frame.len() <= max_frame, "a frame of {} bytes", frame.len());
            let part = split_sections(body_data(frame), 0, 1).concat();
            held.push(part.len());
            joined.extend(part);
        }
        assert_eq!(held, expected, "within {max_frame}");
        assert_eq!(joined, bytes);
    }
    Ok(())
}

/// A header flagged `LAST` leaves the flag to the last frame alone (§13.7): every frame before it is sent without it.
#[test]
fn test_a_header_flagged_last_flags_the_last_frame_alone() -> TestResult {
    let frames =
        Catalog::reply(1, 7, 30, 2, version_bytes()).frames(header().with_flags(FLAG_LAST), 4_096)?;
    let flagged = frames
        .iter()
        .map(|frame| Ok(frame_of(frame)?.header().flags() & FLAG_LAST == FLAG_LAST))
        .collect::<TestResult<Vec<bool>>>()?;
    assert_eq!(flagged, [false, false, true]);
    Ok(())
}

/// A `max_frame` carrying no byte of the version is refused as `FrameTooLarge` naming the least frame that holds the
/// next unit (§2.3, §4.1): the frame header and command header, 256 bytes, the section table of one section, 8, and one
/// byte, 256 + 8 + 1 = 265. Within 265 each frame holds one byte.
#[test]
fn test_a_max_frame_below_the_least_frame_names_it() -> TestResult {
    let reply = Catalog::reply(1, 7, 30, 2, b"UGC".to_vec());
    let refusal = reply.frames(header(), 264).err().context("a refusal")?.0;
    assert_eq!(
        refusal,
        ProtocolErrorEnum::FrameTooLarge {
            size: 265,
            limit: 264
        }
    );
    let frames = reply.frames(header(), 265)?;
    assert_eq!(frames.iter().map(Vec::len).collect::<Vec<_>>(), [265; 3]);
    Ok(())
}
