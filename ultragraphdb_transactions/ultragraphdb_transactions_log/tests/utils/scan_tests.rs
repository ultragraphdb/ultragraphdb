/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scan of a segment's bytes. Segments are built here frame by frame with `encode_round`, whose layout
//! `frame_tests` pins against the tables; every expected offset, position and range is worked out by hand from the
//! frame sizes (a 1,000-byte body is one block, a 5,000-byte body two).

use ultragraphdb_error_log::{LogError, LogErrorEnum};
use ultragraphdb_protocol_wire::{Command, Header, RoundHeader};
use ultragraphdb_traits_log::{Position, RoundRef};
use ultragraphdb_transactions_log::{
    Damage, ScanStart, SegmentScan, encode_round, find_complete_round, scan_segment,
};

use crate::utils::helpers::{
    CLUSTER, FULL_BLOCK, FULL_TWO_BLOCKS, ONE_BLOCK, TWO_BLOCKS, body, frame_checksum,
};

/// A round to lay into a segment: position, parent, incarnation, body length.
#[derive(Clone, Copy)]
struct Spec {
    position: u64,
    parent: u64,
    incarnation: u64,
    len: usize,
}

/// Rounds `from` to `to` of `incarnation`, chained from `parent`, each with a body of `len`.
fn chain(incarnation: u64, from: u64, to: u64, parent: u64, len: usize) -> Vec<Vec<u8>> {
    let mut parent = parent;
    (from..=to)
        .map(|position| {
            let frame = frame_of(Spec {
                position,
                parent,
                incarnation,
                len,
            });
            parent = frame_checksum(&frame);
            frame
        })
        .collect()
}

fn frame_of(spec: Spec) -> Vec<u8> {
    let data = body(spec.position, spec.len);
    let round = RoundRef::new(
        Position::new(spec.position),
        spec.parent,
        spec.position.wrapping_mul(2),
        1,
        &data,
    );
    encode_round(CLUSTER, 0, spec.incarnation, &round)
        .expect("encoded")
        .0
        .as_slice()
        .to_vec()
}

/// A segment of `blocks` blocks holding `frames` from its start, then zeros.
fn segment(blocks: usize, frames: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes: Vec<u8> = frames.concat();
    assert!(bytes.len() <= blocks * 4_096);
    bytes.resize(blocks * 4_096, 0);
    bytes
}

fn start() -> ScanStart {
    ScanStart::new(CLUSTER, 1, Position::ZERO, 0)
}

fn scan(bytes: &[u8]) -> SegmentScan {
    scan_segment(bytes, &start()).expect("scanned")
}

fn positions(scan: &SegmentScan) -> Vec<u64> {
    scan.rounds().iter().map(|r| r.position().get()).collect()
}

/// Writes `checksum` over a header changed in place: XXH3-64 of its bytes 8 to 256, with `xxhash_rust`.
fn reseal(frame: &mut [u8]) {
    let checksum = xxhash_rust::xxh3::xxh3_64(&frame[8..256]);
    frame[..8].copy_from_slice(&checksum.to_le_bytes());
}

#[test]
fn test_a_segment_of_zeros_holds_no_round() {
    let scan = scan(&segment(4, &[]));
    assert!(scan.rounds().is_empty());
    assert_eq!(scan.end(), 0);
    assert_eq!(scan.damage(), None);
}

#[test]
fn test_three_rounds_then_zeros_end_after_the_third() {
    let frames = chain(1, 1, 3, 0, ONE_BLOCK);
    let scan = scan(&segment(16, &frames));
    assert_eq!(positions(&scan), [1, 2, 3]);
    assert_eq!(scan.end(), 12_288);
    assert_eq!(scan.damage(), None);
    for (i, round) in scan.rounds().iter().enumerate() {
        assert_eq!(round.offset(), 4_096 * i as u64);
        assert_eq!(round.size(), 1_256);
        assert_eq!(round.checksum(), frame_checksum(&frames[i]));
        assert_eq!(round.first_version(), 2 * (i as u64 + 1));
        assert_eq!(round.transactions(), 1);
    }
    assert_eq!(scan.rounds()[0].parent(), 0);
    assert_eq!(scan.rounds()[2].parent(), frame_checksum(&frames[1]));
}

#[test]
fn test_rounds_of_mixed_sizes_lie_at_block_boundaries() {
    let one = frame_of(Spec {
        position: 1,
        parent: 0,
        incarnation: 1,
        len: ONE_BLOCK,
    });
    let two = frame_of(Spec {
        position: 2,
        parent: frame_checksum(&one),
        incarnation: 1,
        len: TWO_BLOCKS,
    });
    let three = frame_of(Spec {
        position: 3,
        parent: frame_checksum(&two),
        incarnation: 1,
        len: ONE_BLOCK,
    });
    let scan = scan(&segment(8, &[one, two, three]));
    let offsets: Vec<u64> = scan.rounds().iter().map(|r| r.offset()).collect();
    assert_eq!(offsets, [0, 4_096, 12_288]);
    assert_eq!(scan.rounds()[1].size(), 5_272);
    assert_eq!(scan.end(), 16_384);
}

#[test]
fn test_rounds_filling_the_segment_end_at_its_length() {
    let scan = scan(&segment(4, &chain(1, 1, 4, 0, ONE_BLOCK)));
    assert_eq!(positions(&scan), [1, 2, 3, 4]);
    assert_eq!(scan.end(), 16_384);
    assert_eq!(scan.damage(), None);
}

#[test]
fn test_bytes_short_of_a_block_are_not_read() {
    let mut bytes = segment(2, &chain(1, 1, 2, 0, ONE_BLOCK));
    bytes.extend_from_slice(&[0xFF; 100]);
    let scan = scan(&bytes);
    assert_eq!(positions(&scan), [1, 2]);
    assert_eq!(scan.end(), 8_192);
    assert_eq!(scan.damage(), None);
    for short in [Vec::new(), vec![0xFF; 4_095]] {
        let scan = self::scan(&short);
        assert!(scan.rounds().is_empty());
        assert_eq!((scan.end(), scan.damage()), (0, None));
    }
}

#[test]
fn test_a_round_whose_padding_lies_past_the_bytes_ends_at_its_padded_end() {
    let frames = chain(1, 1, 2, 0, ONE_BLOCK);
    let mut bytes = segment(1, &frames[..1]);
    bytes.extend_from_slice(&frames[1]);
    let scan = scan(&bytes);
    assert_eq!(positions(&scan), [1, 2]);
    assert_eq!(scan.end(), 8_192);
    assert_eq!(scan.damage(), None);
}

/// Each committed case scanned from the fuzz target `scan`'s three starts: the whole input from a fresh log, and the
/// input after its first eight bytes from the checkpoint those bytes choose and from a continued chain.
#[test]
fn test_the_scan_fuzz_regressions_end_within_a_block_of_the_bytes() {
    let cases = ultragraphdb_tool_bdd::fuzz_regressions("scan");
    assert!(!cases.is_empty(), "the committed cases");
    for (name, data) in cases {
        let (checkpoint, rest) = data.split_at(data.len().min(8));
        let mut first = [0u8; 8];
        first[..checkpoint.len()].copy_from_slice(checkpoint);
        let at = Position::new(u64::from(first[0] % 8));
        let starts = [
            (data.as_slice(), start()),
            (
                rest,
                ScanStart::new(CLUSTER, 1, at, u64::from_le_bytes(first)),
            ),
            (rest, start().continuing(Position::new(1), 0)),
        ];
        for (bytes, start) in starts {
            if let Ok(scan) = scan_segment(bytes, &start) {
                assert_eq!(scan.end() % 4_096, 0, "{name}");
                assert!(scan.end() < bytes.len() as u64 + 4_096, "{name}");
            }
        }
    }
}

#[test]
fn test_rounds_of_an_older_incarnation_after_the_current_are_a_clean_end() {
    // A reused segment: incarnation 1 wrote rounds 1 to 6; incarnation 2 wrote its rounds 1 to 3 over the first three.
    let old = chain(1, 1, 6, 0, ONE_BLOCK);
    let new = chain(2, 1, 3, 0, ONE_BLOCK);
    let frames = [new.clone(), old[3..].to_vec()].concat();
    let bytes = segment(8, &frames);
    let scan =
        scan_segment(&bytes, &ScanStart::new(CLUSTER, 2, Position::ZERO, 0)).expect("scanned");
    assert_eq!(positions(&scan), [1, 2, 3]);
    assert_eq!(scan.end(), 12_288);
    assert_eq!(scan.damage(), None);
    // Read as incarnation 1, the same segment's first block is not this log's: a clean end at once.
    let as_old = scan_segment(&bytes, &start()).expect("scanned");
    assert!(as_old.rounds().is_empty());
    assert_eq!((as_old.end(), as_old.damage()), (0, None));
}

#[test]
fn test_a_valid_round_of_another_cluster_is_damage() {
    let data = body(1, 10);
    let round = RoundRef::new(Position::new(1), 0, 2, 1, &data);
    let foreign = encode_round(CLUSTER ^ 1, 0, 1, &round).expect("encoded").0;
    let scan = scan(&segment(4, &[foreign.as_slice().to_vec()]));
    assert!(scan.rounds().is_empty());
    assert_eq!((scan.end(), scan.damage()), (0, Some(Damage::Header)));
}

#[test]
fn test_a_flipped_header_byte_is_header_damage_where_it_lies() {
    // Offsets in each part of the header: checksum, checksum_body, cluster, size, command, reserved, command header.
    for offset in [0usize, 8, 33, 41, 54, 100, 128, 140, 255] {
        let mut frames = chain(1, 1, 3, 0, ONE_BLOCK);
        frames[1][offset] ^= 0x10;
        let scan = scan(&segment(8, &frames));
        assert_eq!(positions(&scan), [1], "offset {offset}");
        assert_eq!(scan.end(), 4_096, "offset {offset}");
        assert_eq!(scan.damage(), Some(Damage::Header), "offset {offset}");
    }
}

#[test]
fn test_a_size_out_of_bounds_is_header_damage_before_its_checksum_is_trusted() {
    // Resealed, so only the size rule refuses them: below a header, past 1 MiB, past the segment's end.
    for size in [0u32, 255, 1_048_577, u32::MAX, 16_385] {
        let mut frames = chain(1, 1, 1, 0, ONE_BLOCK);
        frames[0][40..44].copy_from_slice(&size.to_le_bytes());
        reseal(&mut frames[0]);
        let scan = scan(&segment(4, &frames));
        assert!(scan.rounds().is_empty(), "size {size}");
        assert_eq!(scan.damage(), Some(Damage::Header), "size {size}");
    }
    // A valid two-block round in the segment's last block runs past its end.
    let mut frames = chain(1, 1, 3, 0, ONE_BLOCK);
    frames.push(frame_of(Spec {
        position: 4,
        parent: frame_checksum(&frames[2]),
        incarnation: 1,
        len: TWO_BLOCKS,
    }));
    let mut bytes: Vec<u8> = frames.concat();
    bytes.truncate(16_384);
    let scan = scan(&bytes);
    assert_eq!(positions(&scan), [1, 2, 3]);
    assert_eq!((scan.end(), scan.damage()), (12_288, Some(Damage::Header)));
}

#[test]
fn test_a_frame_of_another_command_is_header_damage() {
    let data = body(1, 10);
    let header = Header::new(CLUSTER, Command::Commit)
        .with_command_header(RoundHeader::new(1, 0, 1, 2, 1).encode());
    let mut frame = header.encode_frame(&data).expect("a frame");
    frame.resize(4_096, 0);
    let scan = scan(&segment(4, &[frame]));
    assert_eq!((scan.end(), scan.damage()), (0, Some(Damage::Header)));
}

#[test]
fn test_a_broken_chain_is_chain_damage_naming_the_round() {
    let frames = chain(1, 1, 3, 0, ONE_BLOCK);
    // Round 2 with another parent; round 3 after round 1; round 1 again after round 1.
    let wrong_parent = frame_of(Spec {
        position: 2,
        parent: 0xDEAD,
        incarnation: 1,
        len: ONE_BLOCK,
    });
    let skipped = frames[2].clone();
    let repeated = frame_of(Spec {
        position: 1,
        parent: frame_checksum(&frames[0]),
        incarnation: 1,
        len: ONE_BLOCK,
    });
    for (second, position) in [(wrong_parent, 2), (skipped, 3), (repeated, 1)] {
        let scan = scan(&segment(4, &[frames[0].clone(), second]));
        assert_eq!(positions(&scan), [1]);
        assert_eq!(scan.end(), 4_096);
        assert_eq!(
            scan.damage(),
            Some(Damage::Chain {
                position: Position::new(position)
            })
        );
    }
}

#[test]
fn test_a_fresh_logs_first_round_with_another_parent_diverges() {
    // A fresh log's checkpoint is position 0 with checksum 0, so its round 1 must name parent 0.
    let frames = chain(1, 1, 1, 0xBEEF, ONE_BLOCK);
    let error = scan_segment(&segment(4, &frames), &start()).expect_err("diverged");
    assert_eq!(diverged(error), ("the first round's parent", 0, 0xBEEF));
}

#[test]
fn test_a_flipped_table_byte_is_chunk_table_damage() {
    let mut frames = chain(1, 1, 2, 0, TWO_BLOCKS);
    // The table is the body's last 16 bytes: frame bytes 5,256 to 5,272.
    frames[1][256 + 5_000 + 3] ^= 0x01;
    let scan = scan(&segment(8, &frames));
    assert_eq!(positions(&scan), [1]);
    assert_eq!(scan.end(), 8_192);
    assert_eq!(
        scan.damage(),
        Some(Damage::ChunkTable {
            position: Position::new(2)
        })
    );
}

#[test]
fn test_a_flipped_chunk_byte_is_named_with_its_range() {
    // Chunk 0 is body bytes 0 to 3,840; chunk 1 of a 5,000-byte body is 3,840 to 5,000.
    for (body_offset, index, start, end) in [
        (10usize, 0, 0, 3_840),
        (3_839, 0, 0, 3_840),
        (3_840, 1, 3_840, 5_000),
        (4_999, 1, 3_840, 5_000),
    ] {
        let mut frames = chain(1, 1, 2, 0, TWO_BLOCKS);
        frames[1][256 + body_offset] ^= 0x80;
        let scan = scan(&segment(8, &frames));
        assert_eq!(positions(&scan), [1]);
        assert_eq!(
            scan.damage(),
            Some(Damage::Chunk {
                position: Position::new(2),
                index,
                start,
                end
            }),
            "body byte {body_offset}"
        );
    }
    let mut frames = chain(1, 1, 1, 0, ONE_BLOCK);
    frames[0][256 + 999] ^= 0x01;
    let scan = scan(&segment(4, &frames));
    assert_eq!(
        scan.damage(),
        Some(Damage::Chunk {
            position: Position::new(1),
            index: 0,
            start: 0,
            end: 1_000
        })
    );
}

#[test]
fn test_padding_after_a_frame_is_not_checked() {
    let mut frames = chain(1, 1, 2, 0, ONE_BLOCK);
    frames[0][4_000] = 0xFF;
    assert_eq!(positions(&scan(&segment(4, &frames))), [1, 2]);
}

#[test]
fn test_the_first_kept_segment_may_start_below_the_checkpoint() {
    // Checkpoint at 7: the segment starts at round 5, and round 7's checksum is the checkpoint's.
    let frames = chain(1, 5, 9, 0x55, ONE_BLOCK);
    let at_seven = frame_checksum(&frames[2]);
    let bytes = segment(8, &frames);
    let scan = scan_segment(
        &bytes,
        &ScanStart::new(CLUSTER, 1, Position::new(7), at_seven),
    )
    .expect("scanned");
    assert_eq!(positions(&scan), [5, 6, 7, 8, 9]);
    assert_eq!(scan.rounds()[0].parent(), 0x55);
    // The segment starts at the round after the checkpoint, its parent the checkpoint's checksum.
    let after = chain(1, 8, 9, 0x77, ONE_BLOCK);
    let scan = scan_segment(
        &segment(4, &after),
        &ScanStart::new(CLUSTER, 1, Position::new(7), 0x77),
    )
    .expect("scanned");
    assert_eq!(positions(&scan), [8, 9]);
}

fn diverged(error: LogError) -> (&'static str, u64, u64) {
    match error.0 {
        LogErrorEnum::Diverged {
            what,
            expected,
            found,
        } => (what, expected, found),
        other => panic!("not diverged: {other:?}"),
    }
}

#[test]
fn test_rounds_contradicting_the_checkpoint_diverge() {
    let frames = chain(1, 5, 9, 0x55, ONE_BLOCK);
    let bytes = segment(8, &frames);
    let at_seven = frame_checksum(&frames[2]);
    // Round 7 with another checksum than the checkpoint's.
    let error = scan_segment(
        &bytes,
        &ScanStart::new(CLUSTER, 1, Position::new(7), 0x1234),
    )
    .expect_err("diverged");
    assert_eq!(
        diverged(error),
        ("the checkpoint's checksum", 0x1234, at_seven)
    );
    // The segment starts past the round after the checkpoint: rounds 5 to 9 with the checkpoint at 3.
    let error = scan_segment(&bytes, &ScanStart::new(CLUSTER, 1, Position::new(3), 0))
        .expect_err("diverged");
    assert_eq!(diverged(error), ("the first round's position", 4, 5));
    // The round after the checkpoint names another parent than the checkpoint's checksum.
    let error = scan_segment(&bytes, &ScanStart::new(CLUSTER, 1, Position::new(4), 0x99))
        .expect_err("diverged");
    assert_eq!(diverged(error), ("the first round's parent", 0x99, 0x55));
}

#[test]
fn test_a_continuing_segment_starts_with_exactly_the_next_round() {
    let frames = chain(1, 5, 6, 0x55, ONE_BLOCK);
    let bytes = segment(4, &frames);
    let base = ScanStart::new(CLUSTER, 1, Position::ZERO, 0);
    let scan = scan_segment(&bytes, &base.continuing(Position::new(5), 0x55)).expect("scanned");
    assert_eq!(positions(&scan), [5, 6]);
    for (position, parent, named) in [(4, 0x55, 5), (5, 0x56, 5), (6, 0x55, 5)] {
        let scan = scan_segment(&bytes, &base.continuing(Position::new(position), parent))
            .expect("scanned");
        assert!(scan.rounds().is_empty());
        assert_eq!(
            scan.damage(),
            Some(Damage::Chain {
                position: Position::new(named)
            })
        );
    }
}

#[test]
fn test_find_complete_round_after_damage() {
    let mut frames = chain(1, 1, 4, 0, ONE_BLOCK);
    frames[1][300] ^= 0x01;
    let bytes = segment(8, &frames);
    assert_eq!(
        find_complete_round(&bytes, 4_096 + 1, CLUSTER, 1, Position::new(2)),
        Some(8_192)
    );
    // From a block boundary, that block is a candidate: round 1 at 0, round 2 is damaged.
    assert_eq!(
        find_complete_round(&bytes, 0, CLUSTER, 1, Position::new(1)),
        Some(0)
    );
    assert_eq!(
        find_complete_round(&bytes, 4_096, CLUSTER, 1, Position::new(1)),
        Some(8_192)
    );
    // Rounds below the position asked for are passed over.
    assert_eq!(
        find_complete_round(&bytes, 0, CLUSTER, 1, Position::new(4)),
        Some(12_288)
    );
    assert_eq!(
        find_complete_round(&bytes, 0, CLUSTER, 1, Position::new(5)),
        None
    );
    assert_eq!(
        find_complete_round(&bytes, 0, CLUSTER, 2, Position::new(1)),
        None
    );
    assert_eq!(
        find_complete_round(&bytes, 0, CLUSTER ^ 1, 1, Position::new(1)),
        None
    );
    assert_eq!(
        find_complete_round(&bytes, 16_384, CLUSTER, 1, Position::new(1)),
        None
    );
    assert_eq!(
        find_complete_round(&bytes, usize::MAX, CLUSTER, 1, Position::new(1)),
        None
    );
    assert_eq!(
        find_complete_round(&[], 0, CLUSTER, 1, Position::new(1)),
        None
    );
}

#[test]
fn test_find_complete_round_needs_the_whole_frame() {
    let frames = chain(1, 1, 1, 0, TWO_BLOCKS);
    let mut bytes = frames[0].clone();
    bytes.truncate(5_271);
    assert_eq!(
        find_complete_round(&bytes, 0, CLUSTER, 1, Position::new(1)),
        None
    );
    bytes = frames[0].clone();
    bytes[256 + 4_000] ^= 0x01;
    assert_eq!(
        find_complete_round(&bytes, 0, CLUSTER, 1, Position::new(1)),
        None
    );
}

#[test]
fn test_the_round_at_the_last_position_ends_the_scan() {
    // A checkpoint just below u64::MAX: the first round may be the last position, and nothing can follow it.
    let frames = chain(1, u64::MAX, u64::MAX, 0x77, ONE_BLOCK);
    let bytes = segment(4, &[frames[0].clone(), frames[0].clone()]);
    let start = ScanStart::new(CLUSTER, 1, Position::new(u64::MAX - 1), 0x77);
    let scan = scan_segment(&bytes, &start).expect("scanned");
    assert_eq!(positions(&scan), [u64::MAX]);
    assert_eq!((scan.end(), scan.damage()), (4_096, None));
}

/// A checkpoint at the last position has no round after it: a first kept segment starting with the checkpoint's own
/// round, `u64::MAX`, whose parent is the round before it, is read, not taken as the round after the checkpoint with
/// the wrong parent.
#[test]
fn test_a_checkpoint_at_the_last_position_has_no_round_after_it() {
    let frames = chain(1, u64::MAX - 1, u64::MAX, 0x55, ONE_BLOCK);
    let last = frame_checksum(&frames[1]);
    // The segment starts at round u64::MAX, its parent round u64::MAX - 1's checksum.
    let bytes = segment(4, &frames[1..]);
    let start = ScanStart::new(CLUSTER, 1, Position::new(u64::MAX), last);
    let scan = scan_segment(&bytes, &start).expect("scanned, not diverged");
    assert_eq!(positions(&scan), [u64::MAX]);
    assert_eq!((scan.end(), scan.damage()), (4_096, None));
    // Its checksum must still be the checkpoint's.
    let error = scan_segment(
        &bytes,
        &ScanStart::new(CLUSTER, 1, Position::new(u64::MAX), last ^ 1),
    )
    .expect_err("diverged");
    assert_eq!(
        diverged(error),
        ("the checkpoint's checksum", last ^ 1, last)
    );
}

/// A Round frame is numbered by its round's position (`PROTOCOL.md` §12): one whose header says another, its checksum
/// sound, is no round of the log and stops the scan as damage.
#[test]
fn test_a_round_numbered_other_than_its_position_is_damage() {
    let mut frames = chain(1, 1, 3, 0, ONE_BLOCK);
    Header::stamp_sequence(&mut frames[1], 7).expect("a header");
    let scan = scan(&segment(8, &frames));
    assert_eq!(positions(&scan), [1]);
    assert_eq!(scan.damage(), Some(Damage::Header));
}

/// The damage a scan must report for a round whose frame `frame` kept only its first `kept` bytes, the rest zeros, as a
/// torn write leaves it, derived from the order the log checks a round in (the header, then the chunk table, then each
/// chunk) and from which bytes the tear changed: `None` when it changed none. `table` is the frame's chunk-table
/// range, `None` for a one-chunk body, whose checksum is its only chunk's; `chunks` the frame ranges of the chunks.
fn torn_damage(
    frame: &[u8],
    kept: usize,
    position: u64,
    table: Option<std::ops::Range<usize>>,
    chunks: &[(usize, std::ops::Range<usize>)],
) -> Option<Option<Damage>> {
    // Whether the tear zeroed a nonzero byte of `range`; a range wholly kept was not changed.
    let changed = |range: std::ops::Range<usize>| {
        frame
            .get(kept.max(range.start)..range.end)
            .is_some_and(|bytes| bytes.iter().any(|b| *b != 0))
    };
    let position = Position::new(position);
    if kept == 0 {
        // Nothing kept: a block of zeros, the clean end of the segment.
        return Some(None);
    }
    if changed(0..256) {
        return Some(Some(Damage::Header));
    }
    if let Some(table) = table
        && changed(table)
    {
        return Some(Some(Damage::ChunkTable { position }));
    }
    for (index, range) in chunks {
        if changed(range.clone()) {
            // A chunk's range is named in body bytes: its frame range less the 256-byte header.
            return Some(Some(Damage::Chunk {
                position,
                index: *index,
                start: range.start - 256,
                end: range.end - 256,
            }));
        }
    }
    None
}

/// A torn tail at every byte boundary of its frame: a one-block round (one 3,840-byte chunk, no table) and a two-block
/// round (chunks of body bytes 0 to 3,840 and 3,840 to 7,904, then its 16-byte table, frame bytes 8,176 to 8,192). Each
/// tear is damage named as the checks' order says, a clean end when nothing was kept, and the whole round only when
/// the tear changed no byte; the round before it is always read.
#[test]
fn test_a_round_torn_at_every_byte_is_damage_or_a_clean_end() {
    let chunk = |index, start, end| {
        Some(Damage::Chunk {
            position: Position::new(2),
            index,
            start,
            end,
        })
    };
    let table_damage = Some(Damage::ChunkTable {
        position: Position::new(2),
    });
    // Each case with the outcomes its tears must reach, so the rule is never checked on one branch alone: the clean
    // end (nothing kept), header damage (a tear inside the command header's nonzero bytes), then the body's damage.
    let cases: [(
        usize,
        Option<std::ops::Range<usize>>,
        Vec<(usize, std::ops::Range<usize>)>,
        Vec<Option<Damage>>,
    ); 2] = [
        (
            FULL_BLOCK,
            None,
            vec![(0, 256..4_096)],
            vec![None, Some(Damage::Header), chunk(0, 0, 3_840)],
        ),
        (
            FULL_TWO_BLOCKS,
            Some(8_176..8_192),
            vec![(0, 256..4_096), (1, 4_096..8_160)],
            vec![None, Some(Damage::Header), table_damage],
        ),
    ];
    for (len, table, chunks, reached) in cases {
        let mut outcomes = std::collections::BTreeSet::new();
        let frames = chain(1, 1, 2, 0, ONE_BLOCK);
        let torn_frame = frame_of(Spec {
            position: 2,
            parent: frame_checksum(&frames[0]),
            incarnation: 1,
            len,
        });
        let size = torn_frame.len();
        for kept in 0..size {
            let mut torn = torn_frame.clone();
            torn[kept..].fill(0);
            let scan = scan(&segment(4, &[frames[0].clone(), torn]));
            match torn_damage(&torn_frame, kept, 2, table.clone(), &chunks) {
                Some(damage) => {
                    outcomes.insert(format!("{damage:?}"));
                    assert_eq!(positions(&scan), [1], "len {len}, kept {kept}");
                    assert_eq!(
                        (scan.end(), scan.damage()),
                        (4_096, damage),
                        "len {len}, kept {kept}"
                    );
                }
                None => assert_eq!(positions(&scan), [1, 2], "len {len}, kept {kept}"),
            }
        }
        for outcome in reached {
            assert!(
                outcomes.contains(&format!("{outcome:?}")),
                "len {len}: {outcome:?} never reached in {outcomes:?}"
            );
        }
    }
}
