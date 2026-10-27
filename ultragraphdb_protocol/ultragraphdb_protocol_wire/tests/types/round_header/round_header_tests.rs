/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{COMMAND_HEADER_SIZE, RoundHeader};

/// The command header laid out by hand from the type's table: position at 0, parent at 8, incarnation at 16, first
/// version at 24, transactions at 32, zeros from 36.
fn by_hand(position: u64, parent: u64, incarnation: u64, first: u64, count: u32) -> [u8; 128] {
    let mut bytes = [0u8; 128];
    bytes[0..8].copy_from_slice(&position.to_le_bytes());
    bytes[8..16].copy_from_slice(&parent.to_le_bytes());
    bytes[16..24].copy_from_slice(&incarnation.to_le_bytes());
    bytes[24..32].copy_from_slice(&first.to_le_bytes());
    bytes[32..36].copy_from_slice(&count.to_le_bytes());
    bytes
}

#[test]
fn test_fields_read_back_as_given() {
    let header = RoundHeader::new(7, 0xA1B2_C3D4_E5F6_0718, 2, 40, 3);
    assert_eq!(header.position(), 7);
    assert_eq!(header.parent(), 0xA1B2_C3D4_E5F6_0718);
    assert_eq!(header.incarnation(), 2);
    assert_eq!(header.first_version(), 40);
    assert_eq!(header.transactions(), 3);
}

#[test]
fn test_encode_lays_out_the_table() {
    // Every field distinct and every byte of each nonzero, so a field written at a neighbour's offset is caught.
    let cases = [
        (7, 0xA1B2_C3D4_E5F6_0718, 2, 40, 3),
        (
            0x0102_0304_0506_0708,
            0x1112_1314_1516_1718,
            0x2122_2324_2526_2728,
            0x3132_3334_3536_3738,
            0x4142_4344,
        ),
        (u64::MAX, u64::MAX, u64::MAX, u64::MAX, u32::MAX),
        (0, 0, 0, 0, 0),
    ];
    for (position, parent, incarnation, first, count) in cases {
        let header = RoundHeader::new(position, parent, incarnation, first, count);
        assert_eq!(
            header.encode(),
            by_hand(position, parent, incarnation, first, count)
        );
        assert_eq!(header.encode().len(), COMMAND_HEADER_SIZE);
    }
}

#[test]
fn test_decode_reads_the_table() {
    let bytes = by_hand(
        0x0102_0304_0506_0708,
        0x1112_1314_1516_1718,
        0x2122_2324_2526_2728,
        0x3132_3334_3536_3738,
        0x4142_4344,
    );
    let header = RoundHeader::decode(&bytes);
    assert_eq!(header.position(), 0x0102_0304_0506_0708);
    assert_eq!(header.parent(), 0x1112_1314_1516_1718);
    assert_eq!(header.incarnation(), 0x2122_2324_2526_2728);
    assert_eq!(header.first_version(), 0x3132_3334_3536_3738);
    assert_eq!(header.transactions(), 0x4142_4344);
}

#[test]
fn test_decode_ignores_the_reserved_bytes() {
    let mut bytes = by_hand(7, 1, 2, 40, 3);
    bytes[36..].fill(0xFF);
    assert_eq!(
        RoundHeader::decode(&bytes),
        RoundHeader::new(7, 1, 2, 40, 3)
    );
}
