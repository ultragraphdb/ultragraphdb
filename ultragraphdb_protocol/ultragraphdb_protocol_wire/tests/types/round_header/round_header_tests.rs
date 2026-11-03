/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{COMMAND_HEADER_SIZE, RoundHeader};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The command header laid out by hand from PROTOCOL.md §12.1's table: position at 0, parent at 8, incarnation at
/// 16, first version at 24, transactions at 32, derivation at 36, zeros from 40.
fn by_hand(fields: (u64, u64, u64, u64, u32, u32)) -> [u8; 128] {
    let (position, parent, incarnation, first, count, derivation) = fields;
    let mut bytes = [0u8; 128];
    bytes[0..8].copy_from_slice(&position.to_le_bytes());
    bytes[8..16].copy_from_slice(&parent.to_le_bytes());
    bytes[16..24].copy_from_slice(&incarnation.to_le_bytes());
    bytes[24..32].copy_from_slice(&first.to_le_bytes());
    bytes[32..36].copy_from_slice(&count.to_le_bytes());
    bytes[36..40].copy_from_slice(&derivation.to_le_bytes());
    bytes
}

fn derivation(n: u32) -> TestResult<NonZeroU32> {
    NonZeroU32::new(n).context("a derivation from 1")
}

#[test]
fn test_fields_read_back_as_given() -> TestResult {
    let header = RoundHeader::new(7, 0xA1B2_C3D4_E5F6_0718, 2, 40, 3, derivation(5)?);
    assert_eq!(header.position(), 7);
    assert_eq!(header.parent(), 0xA1B2_C3D4_E5F6_0718);
    assert_eq!(header.incarnation(), 2);
    assert_eq!(header.first_version(), 40);
    assert_eq!(header.transactions(), 3);
    assert_eq!(header.derivation().get(), 5);
    Ok(())
}

#[test]
fn test_encode_lays_out_the_table_and_decode_reads_it() -> TestResult {
    // Every field distinct and every byte of each nonzero, so a field written at a neighbour's offset is caught.
    let cases = [
        (7, 0xA1B2_C3D4_E5F6_0718, 2, 40, 3, 1),
        (
            0x0102_0304_0506_0708,
            0x1112_1314_1516_1718,
            0x2122_2324_2526_2728,
            0x3132_3334_3536_3738,
            0x4142_4344,
            0x5152_5354,
        ),
        (u64::MAX, u64::MAX, u64::MAX, u64::MAX, u32::MAX, u32::MAX),
        (0, 0, 0, 0, 0, 1),
    ];
    for fields in cases {
        let (position, parent, incarnation, first, count, d) = fields;
        let header = RoundHeader::new(position, parent, incarnation, first, count, derivation(d)?);
        assert_eq!(header.encode(), by_hand(fields));
        assert_eq!(header.encode().len(), COMMAND_HEADER_SIZE);
        assert_eq!(RoundHeader::decode(&by_hand(fields))?, header);
    }
    Ok(())
}

#[test]
fn test_decode_refuses_unused_bytes_then_a_derivation_of_zero() -> TestResult {
    for at in [40usize, 127] {
        let mut bytes = by_hand((7, 1, 2, 40, 3, 1));
        bytes[at] = 1;
        assert_eq!(
            RoundHeader::decode(&bytes).err().context("a refusal")?.0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let zero = by_hand((7, 1, 2, 40, 3, 0));
    assert_eq!(
        RoundHeader::decode(&zero).err().context("a refusal")?.0,
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "derivation",
            value: 0
        }
    );
    // The unused bytes are checked first.
    let mut both = zero;
    both[64] = 9;
    assert_eq!(
        RoundHeader::decode(&both).err().context("a refusal")?.0,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 64 }
    );
    Ok(())
}
