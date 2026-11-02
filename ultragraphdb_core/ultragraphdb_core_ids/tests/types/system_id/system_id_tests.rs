/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{
    Bucket, CanonicalKey, IdError, IdErrorEnum, Namespace, Sequence, SystemId, TypePrefix,
};
use ultragraphdb_tool_bdd::TestResult;

// Expected bytes by hand from the layout: type prefix (2 bytes), sequence (8), bucket (3), reserved (3), big-endian.

fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The bytes of (3, 0xAB_CDEF, 42), the scenario "The fields pack and unpack".
const PACKED: [u8; 16] = [
    0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2A, 0xAB, 0xCD, 0xEF, 0x00, 0x00, 0x00,
];

/// `PACKED` as an unsigned 128-bit value, its bytes read in order.
const PACKED_U128: u128 = 0x0003_0000_0000_0000_002A_ABCD_EF00_0000;

#[test]
fn test_the_fields_pack_and_unpack() -> TestResult {
    let packed = id(3, 0xAB_CDEF, 42)?;
    assert_eq!(packed.to_be_bytes(), PACKED);
    assert_eq!(packed.to_u128(), PACKED_U128);
    assert_eq!(packed.type_prefix().get(), 3);
    assert_eq!(packed.bucket().get(), 0xAB_CDEF);
    assert_eq!(packed.sequence().get(), 42);
    assert_eq!(SystemId::decode(&PACKED)?, packed);
    assert_eq!(SystemId::from_u128(PACKED_U128)?, packed);
    Ok(())
}

#[test]
fn test_the_largest_prefix_with_bucket_zero_and_sequence_one() -> TestResult {
    let packed = id(65_535, 0, 1)?;
    let bytes = [0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0x01, 0, 0, 0, 0, 0, 0];
    assert_eq!(packed.to_u128(), 0xFFFF_0000_0000_0000_0001_0000_0000_0000);
    assert_eq!(packed.to_be_bytes(), bytes);
    assert_eq!(SystemId::decode(&bytes)?, packed);
    assert_eq!(
        (
            packed.type_prefix().get(),
            packed.bucket().get(),
            packed.sequence().get()
        ),
        (65_535, 0, 1)
    );
    Ok(())
}

#[test]
fn test_prefix_one_with_the_largest_bucket_and_sequence() -> TestResult {
    let packed = id(1, 0xFF_FFFF, u64::MAX)?;
    let bytes = [
        0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0,
    ];
    assert_eq!(packed.to_be_bytes(), bytes);
    assert_eq!(packed.to_u128(), 0x0001_FFFF_FFFF_FFFF_FFFF_FFFF_FF00_0000);
    assert_eq!(SystemId::decode(&bytes)?, packed);
    assert_eq!(packed.sequence().get(), u64::MAX);
    assert_eq!(packed.bucket(), Bucket::MAX);
    Ok(())
}

#[test]
fn test_each_reserved_bit_set_alone_is_refused() {
    for bit in 0..24u32 {
        let value = PACKED_U128 | (1u128 << bit);
        let expected = Err(IdError(IdErrorEnum::ReservedBits { bits: 1 << bit }));
        assert_eq!(SystemId::from_u128(value), expected, "bit {bit}");
        assert_eq!(
            SystemId::decode(&value.to_be_bytes()),
            expected,
            "bit {bit}"
        );
    }
}

#[test]
fn test_every_reserved_bit_set_is_refused_naming_them() {
    let mut bytes = PACKED;
    bytes[13..].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
    assert_eq!(
        SystemId::decode(&bytes),
        Err(IdError(IdErrorEnum::ReservedBits { bits: 0xFF_FFFF }))
    );
}

#[test]
fn test_a_zero_type_prefix_is_refused() {
    let mut bytes = PACKED;
    bytes[1] = 0;
    assert_eq!(
        SystemId::decode(&bytes),
        Err(IdError(IdErrorEnum::ZeroTypePrefix))
    );
    assert_eq!(
        SystemId::from_u128(0x0000_0000_0000_0000_002A_ABCD_EF00_0000),
        Err(IdError(IdErrorEnum::ZeroTypePrefix))
    );
}

#[test]
fn test_a_zero_sequence_is_refused() {
    // The sequence's low byte, the only nonzero one of bytes 2 to 9; the bucket stays 0xAB_CDEF.
    let mut bytes = PACKED;
    bytes[9] = 0;
    assert_eq!(
        SystemId::decode(&bytes),
        Err(IdError(IdErrorEnum::ZeroSequence))
    );
    assert_eq!(
        SystemId::from_u128(0x0003_0000_0000_0000_0000_ABCD_EF00_0000),
        Err(IdError(IdErrorEnum::ZeroSequence))
    );
}

#[test]
fn test_the_reserved_bits_are_checked_first_then_the_prefix() {
    // Sixteen bytes whose only nonzero byte is the last: every field is invalid, the reserved bits are named.
    let mut bytes = [0u8; 16];
    bytes[15] = 0x01;
    assert_eq!(
        SystemId::decode(&bytes),
        Err(IdError(IdErrorEnum::ReservedBits { bits: 1 }))
    );
    // A zero prefix and a zero sequence: the prefix is named.
    assert_eq!(
        SystemId::decode(&[0u8; 16]),
        Err(IdError(IdErrorEnum::ZeroTypePrefix))
    );
    assert_eq!(
        SystemId::from_u128(0),
        Err(IdError(IdErrorEnum::ZeroTypePrefix))
    );
}

#[test]
fn test_bytes_of_another_length_are_refused() {
    for len in [0usize, 1, 15, 17, 32] {
        assert_eq!(
            SystemId::decode(&vec![0x01; len]),
            Err(IdError(IdErrorEnum::Length {
                what: "a system ID",
                expected: 16,
                found: len,
            })),
            "{len} bytes"
        );
    }
}

#[test]
fn test_ids_of_one_type_sort_together() -> TestResult {
    let mut ids = [id(2, 9, 1)?, id(1, 900, 7)?, id(1, 3, 99)?];
    ids.sort();
    assert_eq!(ids, [id(1, 900, 7)?, id(1, 3, 99)?, id(2, 9, 1)?]);
    let mut bytes = [
        id(2, 9, 1)?.to_be_bytes(),
        id(1, 900, 7)?.to_be_bytes(),
        id(1, 3, 99)?.to_be_bytes(),
    ];
    bytes.sort();
    assert_eq!(
        bytes,
        [
            id(1, 900, 7)?.to_be_bytes(),
            id(1, 3, 99)?.to_be_bytes(),
            id(2, 9, 1)?.to_be_bytes()
        ]
    );
    Ok(())
}

#[test]
fn test_the_order_is_type_then_sequence_then_bucket() -> TestResult {
    // Each pair differs in the field named, and every field below it runs the other way.
    let pairs = [
        // The type decides over the largest sequence and bucket.
        (id(1, 0xFF_FFFF, u64::MAX)?, id(2, 0, 1)?),
        // Within a type, the sequence decides over the bucket.
        (id(1, 0xFF_FFFF, 5)?, id(1, 0, 6)?),
        (id(1, 0xFF_FFFF, 1 << 31)?, id(1, 1, 1 << 32)?),
        // Within a type and sequence, the bucket decides.
        (id(1, 5, 7)?, id(1, 6, 7)?),
    ];
    for (lower, higher) in pairs {
        assert!(lower < higher, "{lower:?} < {higher:?}");
        assert!(lower.to_be_bytes() < higher.to_be_bytes(), "{lower:?}");
        assert!(lower.to_u128() < higher.to_u128(), "{lower:?}");
    }
    Ok(())
}

#[test]
fn test_a_resource_takes_prefix_one_and_its_derived_bucket() -> TestResult {
    // buckets.json, "the scenario: namespace 0": 0xC7F1AB; "namespace 1": 0xF3F1AB.
    let key = CanonicalKey::iri("http://ex.org/a")?;
    let resource = SystemId::resource(Namespace::new(0), &key, Sequence::new(5)?);
    assert_eq!(resource, id(1, 0xC7_F1AB, 5)?);
    let other = SystemId::resource(Namespace::new(1), &key, Sequence::new(6)?);
    assert_eq!(other, id(1, 0xF3_F1AB, 6)?);
    Ok(())
}

#[test]
fn test_a_statement_takes_prefix_two_and_its_subjects_bucket() -> TestResult {
    let statement = SystemId::statement(id(1, 0xC7_F1AB, 5)?, Sequence::new(9)?);
    assert_eq!(statement, id(2, 0xC7_F1AB, 9)?);
    // A subject of another type and bucket: the statement keeps prefix 2 and takes that bucket.
    let of_declared = SystemId::statement(id(700, 0x12_3456, 77)?, Sequence::new(8)?);
    assert_eq!(of_declared, id(2, 0x12_3456, 8)?);
    Ok(())
}
