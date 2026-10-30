/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::Superblock;

/// The record laid out by hand from Superblock's table, its checksum computed with `xxhash_rust` directly.
fn by_hand(
    sequence: u64,
    cluster: u64,
    incarnation: u64,
    checkpoint: u64,
    checksum: u64,
    version: u64,
    first: u64,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"UGWL");
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    for field in [
        sequence,
        cluster,
        incarnation,
        checkpoint,
        checksum,
        version,
        first,
    ] {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    let sum = xxhash_rust::xxh3::xxh3_64(&bytes);
    bytes.extend_from_slice(&sum.to_le_bytes());
    bytes
}

fn sample() -> Superblock {
    // Every field distinct and every byte of each nonzero.
    Superblock::new(
        0x0102_0304_0506_0708,
        0x1112_1314_1516_1718,
        0x2122_2324_2526_2728,
        Position::new(0x3132_3334_3536_3738),
        0x4142_4344_4546_4748,
        0x6162_6364_6566_6768,
        0x5152_5354_5556_5758,
    )
}

#[test]
fn test_fields_read_back_as_given() {
    let record = sample();
    assert_eq!(record.sequence(), 0x0102_0304_0506_0708);
    assert_eq!(record.cluster(), 0x1112_1314_1516_1718);
    assert_eq!(record.incarnation(), 0x2122_2324_2526_2728);
    assert_eq!(record.checkpoint(), Position::new(0x3132_3334_3536_3738));
    assert_eq!(record.checkpoint_checksum(), 0x4142_4344_4546_4748);
    assert_eq!(record.checkpoint_version(), 0x6162_6364_6566_6768);
    assert_eq!(record.first_segment(), 0x5152_5354_5556_5758);
}

#[test]
fn test_encode_lays_out_the_table() {
    assert_eq!(Superblock::SIZE, 72);
    let expected = by_hand(
        0x0102_0304_0506_0708,
        0x1112_1314_1516_1718,
        0x2122_2324_2526_2728,
        0x3132_3334_3536_3738,
        0x4142_4344_4546_4748,
        0x6162_6364_6566_6768,
        0x5152_5354_5556_5758,
    );
    assert_eq!(sample().encode().as_slice(), expected.as_slice());
    let zero = by_hand(0, 0, 0, 0, 0, 0, 0);
    assert_eq!(
        Superblock::new(0, 0, 0, Position::ZERO, 0, 0, 0)
            .encode()
            .as_slice(),
        zero.as_slice()
    );
}

#[test]
fn test_decode_reads_a_record_written_by_hand() {
    let bytes = by_hand(5, 6, 7, 8, 9, 11, 10);
    assert_eq!(
        Superblock::decode(&bytes),
        Some(Superblock::new(5, 6, 7, Position::new(8), 9, 11, 10))
    );
}

#[test]
fn test_every_bit_flip_is_refused() {
    let bytes = sample().encode();
    for byte in 0..Superblock::SIZE {
        for bit in 0..8 {
            let mut flipped = bytes;
            flipped[byte] ^= 1 << bit;
            assert_eq!(Superblock::decode(&flipped), None, "byte {byte} bit {bit}");
        }
    }
}

#[test]
fn test_lengths_other_than_a_record_are_refused() {
    let bytes = sample().encode();
    assert_eq!(Superblock::decode(&[]), None);
    assert_eq!(Superblock::decode(&bytes[..71]), None);
    let mut long = bytes.to_vec();
    long.push(0);
    assert_eq!(Superblock::decode(&long), None);
    assert_eq!(Superblock::decode(&[0u8; 72]), None);
    assert_eq!(
        Superblock::decode(&bytes[..64]),
        None,
        "a version 1 record's length"
    );
}

#[test]
fn test_a_resealed_record_of_another_version_or_with_reserved_bytes_is_refused() {
    // The checksum is made to match, so the version and reserved rules alone refuse these.
    for (offset, value) in [(4usize, 1u8), (4, 3), (6, 1), (7, 1), (0, b'X')] {
        let mut bytes = by_hand(5, 6, 7, 8, 9, 11, 10);
        bytes[offset] = value;
        let sum = xxhash_rust::xxh3::xxh3_64(&bytes[..64]);
        bytes[64..].copy_from_slice(&sum.to_le_bytes());
        assert_eq!(Superblock::decode(&bytes), None, "offset {offset}");
    }
}

#[test]
fn test_odd_sequences_go_to_copy_a_and_even_to_b() {
    let at = |sequence| Superblock::new(sequence, 0, 0, Position::ZERO, 0, 0, 0).copy();
    assert_eq!(at(1), 'a');
    assert_eq!(at(2), 'b');
    assert_eq!(at(3), 'a');
    assert_eq!(at(0), 'b');
    assert_eq!(at(u64::MAX), 'a');
}
