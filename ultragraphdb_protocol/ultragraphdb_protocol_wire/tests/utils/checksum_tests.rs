/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;

use ultragraphdb_protocol_wire::{xxh3_64, xxh3_64_with_seed};

use crate::utils::golden::{GOLDEN, READ, sanity_buffer};

/// The lines of `testdata/xxh3/sanity_test_vectors_xxh3_64.txt`: length, seed and hash, from xxHash v0.8.3's
/// `tests/sanity_test_vectors.h` (the file's header names the commit and blob).
fn sanity_vectors() -> Vec<(usize, u64, u64)> {
    let path = ultragraphdb_tool_bdd::testdata_dir()
        .join("xxh3")
        .join("sanity_test_vectors_xxh3_64.txt");
    let text = fs::read_to_string(&path).expect("the sanity vectors are readable");
    let hex = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex");
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split(' ').collect();
            (
                fields[0].parse().expect("a length"),
                hex(fields[1]),
                hex(fields[2]),
            )
        })
        .collect()
}

#[test]
fn test_xxh3_64_matches_the_reference_sanity_vectors_at_seed_zero() {
    let buffer = sanity_buffer(4_161);
    let vectors: Vec<_> = sanity_vectors()
        .into_iter()
        .filter(|(_, seed, _)| *seed == 0)
        .collect();
    // 607 lengths at seed 0: 0 to 300, every 13th to 4,160, and 3,838 to 3,842 and 4,094 to 4,098.
    assert_eq!(vectors.len(), 607);
    for (len, _, hash) in vectors {
        assert_eq!(xxh3_64(&buffer[..len]), hash, "length {len}");
    }
}

#[test]
fn test_xxh3_64_with_seed_matches_the_reference_sanity_vectors_at_both_seeds() {
    let buffer = sanity_buffer(4_161);
    let vectors = sanity_vectors();
    assert_eq!(vectors.len(), 1_214);
    for (len, seed, hash) in vectors {
        assert_eq!(
            xxh3_64_with_seed(&buffer[..len], seed),
            hash,
            "length {len}, seed {seed:#x}"
        );
    }
}

#[test]
fn test_leaves_are_seeded_with_their_chunk_index() {
    // The leaves the reference implementation computed for the golden Read frame's six chunks, seeds 0 to 5.
    let data = READ.data();
    let bounds = [0, 3_840, 7_936, 12_032, 16_128, 20_224, 20_432];
    for (i, leaf) in READ.leaves.iter().enumerate() {
        let chunk = &data[bounds[i]..bounds[i + 1]];
        assert_eq!(xxh3_64_with_seed(chunk, i as u64), *leaf, "chunk {i}");
        assert_ne!(xxh3_64_with_seed(chunk, i as u64 + 1), *leaf, "chunk {i}");
    }
}

#[test]
fn test_header_checksums_match_the_reference_over_bytes_8_to_256() {
    for golden in GOLDEN {
        let bytes = golden.bytes();
        assert_eq!(xxh3_64(&bytes[8..256]), golden.checksum, "{}", golden.name);
        assert_eq!(
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            golden.checksum,
            "{} stores its checksum little-endian",
            golden.name
        );
    }
}
