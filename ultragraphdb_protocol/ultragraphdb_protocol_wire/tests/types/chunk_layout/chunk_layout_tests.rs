/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::ChunkLayout;

/// (data length, chunks, table length, body length), worked by hand: one chunk up to 3,840 bytes, one more per
/// 4,096 or part of one, a table of 8 bytes a chunk past the first.
const CASES: [(usize, usize, usize, usize); 10] = [
    (0, 1, 0, 0),
    (1, 1, 0, 1),
    (100, 1, 0, 100),
    (3_840, 1, 0, 3_840),
    (3_841, 2, 16, 3_857),
    (7_936, 2, 16, 7_952),
    (7_937, 3, 24, 7_961),
    (12_032, 3, 24, 12_056),
    (20_432, 6, 48, 20_480),
    (1_046_272, 256, 2_048, 1_048_320),
];

#[test]
fn test_of_data_splits_as_worked_by_hand() {
    for (data_len, chunks, table_len, body_len) in CASES {
        let layout = ChunkLayout::of_data(data_len);
        assert_eq!(layout.data_len(), data_len, "{data_len}");
        assert_eq!(layout.chunks(), chunks, "{data_len}");
        assert_eq!(layout.table_len(), table_len, "{data_len}");
        assert_eq!(layout.body_len(), body_len, "{data_len}");
    }
}

#[test]
fn test_of_body_finds_the_split_worked_by_hand() {
    for (data_len, chunks, _, body_len) in CASES {
        let layout = ChunkLayout::of_body(body_len).expect("a valid body length");
        assert_eq!(
            (layout.data_len(), layout.chunks()),
            (data_len, chunks),
            "{body_len}"
        );
    }
}

#[test]
fn test_lengths_between_splits_are_refused() {
    // 3,841 to 3,856: past one chunk, short of two chunks and their 16-byte table. 7,953 to 7,960: past two full
    // chunks and their table, short of a third chunk's first byte and a 24-byte table.
    for len in (3_841..=3_856).chain(7_953..=7_960) {
        assert_eq!(
            ChunkLayout::of_body(len),
            Err(ProtocolError(ProtocolErrorEnum::BodyLength { len }))
        );
    }
}

/// Every valid body length up to `max_data` bytes of chunks, mapped to its data length, found by walking data
/// lengths one at a time and counting chunks as each boundary is crossed: an oracle that shares no arithmetic with
/// the layout.
fn oracle(max_data: usize) -> HashMap<usize, usize> {
    let mut chunks = 1;
    let mut next_boundary = 3_840;
    let mut valid = HashMap::new();
    for data_len in 0..=max_data {
        if data_len > next_boundary {
            chunks += 1;
            next_boundary += 4_096;
        }
        let table = if chunks > 1 { 8 * chunks } else { 0 };
        assert!(
            valid.insert(data_len + table, data_len).is_none(),
            "two splits of one length"
        );
    }
    valid
}

#[test]
fn test_every_body_length_to_one_mebibyte_matches_the_oracle() {
    let valid = oracle(1_048_320);
    for body_len in 0..=1_048_320 {
        match valid.get(&body_len) {
            Some(&data_len) => {
                let layout = ChunkLayout::of_body(body_len).expect("the oracle's length");
                assert_eq!(layout.data_len(), data_len, "{body_len}");
                assert_eq!(layout.body_len(), body_len, "{body_len}");
                assert_eq!(layout, ChunkLayout::of_data(data_len), "{body_len}");
            }
            None => assert_eq!(
                ChunkLayout::of_body(body_len).map_err(|e| e.0),
                Err(ProtocolErrorEnum::BodyLength { len: body_len }),
                "{body_len}"
            ),
        }
    }
}

#[test]
fn test_chunk_ranges_follow_the_frames_blocks() {
    // The ranges the reference generator printed for the golden Read frame's 20,432 bytes.
    let layout = ChunkLayout::of_data(20_432);
    let want = [
        0..3_840,
        3_840..7_936,
        7_936..12_032,
        12_032..16_128,
        16_128..20_224,
        20_224..20_432,
    ];
    for (i, range) in want.into_iter().enumerate() {
        assert_eq!(layout.chunk_range(i), Some(range), "chunk {i}");
    }
    assert_eq!(layout.chunk_range(6), None);
    assert_eq!(layout.chunk_range(usize::MAX), None);
}

#[test]
fn test_chunk_ranges_at_the_edges() {
    let empty = ChunkLayout::of_data(0);
    assert_eq!(empty.chunk_range(0), Some(0..0));
    assert_eq!(empty.chunk_range(1), None);
    let one = ChunkLayout::of_data(3_840);
    assert_eq!(one.chunk_range(0), Some(0..3_840));
    assert_eq!(one.chunk_range(1), None);
    let two = ChunkLayout::of_data(3_841);
    assert_eq!(two.chunk_range(0), Some(0..3_840));
    assert_eq!(two.chunk_range(1), Some(3_840..3_841));
    assert_eq!(two.chunk_range(2), None);
    let full = ChunkLayout::of_data(7_936);
    assert_eq!(full.chunk_range(1), Some(3_840..7_936));
    assert_eq!(full.chunk_range(2), None);
    let small = ChunkLayout::of_data(100);
    assert_eq!(small.chunk_range(0), Some(0..100));
}

#[test]
fn test_lengths_past_any_frame_do_not_overflow() {
    // No frame holds such a body; the layout still answers without overflowing, and any split it gives adds up.
    for len in [
        usize::MAX,
        usize::MAX - 255,
        usize::MAX - 4_104,
        usize::MAX / 2,
    ] {
        match ChunkLayout::of_body(len) {
            Ok(layout) => assert_eq!(layout.body_len(), len),
            Err(e) => assert_eq!(e.0, ProtocolErrorEnum::BodyLength { len }),
        }
    }
}

#[test]
fn test_a_layout_of_any_data_length_answers_without_overflowing() {
    // usize::MAX bytes take 1 + ceil((usize::MAX - 3,840) / 4,096) chunks; no slice is that long, and the body's
    // length saturates rather than wrapping.
    let layout = ChunkLayout::of_data(usize::MAX);
    assert_eq!(layout.chunks(), 1 + (usize::MAX - 3_840).div_ceil(4_096));
    assert_eq!(layout.body_len(), usize::MAX);
    assert_eq!(layout.chunk_range(layout.chunks()), None);
    let last = layout
        .chunk_range(layout.chunks() - 1)
        .expect("the last chunk");
    assert_eq!(last.end, usize::MAX);
}
