/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};

#[test]
fn test_each_constructor_builds_its_variant_with_its_values() {
    let cases = [
        (
            ProtocolError::ShortHeader(255),
            ProtocolErrorEnum::ShortHeader { len: 255 },
        ),
        (
            ProtocolError::HeaderChecksum(1, 2),
            ProtocolErrorEnum::HeaderChecksum {
                stored: 1,
                computed: 2,
            },
        ),
        (
            ProtocolError::UnknownCommand(18),
            ProtocolErrorEnum::UnknownCommand { command: 18 },
        ),
        (
            ProtocolError::ReservedNotZero(66),
            ProtocolErrorEnum::ReservedNotZero { offset: 66 },
        ),
        (
            ProtocolError::WrongCluster(3, 4),
            ProtocolErrorEnum::WrongCluster {
                expected: 3,
                found: 4,
            },
        ),
        (
            ProtocolError::SizeBelowHeader(255),
            ProtocolErrorEnum::SizeBelowHeader { size: 255 },
        ),
        (
            ProtocolError::FrameTooLarge(1_048_577, 1_048_576),
            ProtocolErrorEnum::FrameTooLarge {
                size: 1_048_577,
                limit: 1_048_576,
            },
        ),
        (
            ProtocolError::LengthMismatch(356, 355),
            ProtocolErrorEnum::LengthMismatch {
                size: 356,
                len: 355,
            },
        ),
        (
            ProtocolError::BodyLength(3_841),
            ProtocolErrorEnum::BodyLength { len: 3_841 },
        ),
        (
            ProtocolError::ChunkTable(5, 6),
            ProtocolErrorEnum::ChunkTable {
                stored: 5,
                computed: 6,
            },
        ),
        (
            ProtocolError::Chunk(3, 12_032, 16_128),
            ProtocolErrorEnum::Chunk {
                index: 3,
                start: 12_032,
                end: 16_128,
            },
        ),
    ];
    for (error, want) in cases {
        assert_eq!(error.0, want);
    }
}

#[test]
fn test_argument_order_is_kept_where_two_values_share_a_type() {
    // Distinct values in each position, so a constructor swapping its arguments fails.
    assert_ne!(
        ProtocolError::HeaderChecksum(1, 2),
        ProtocolError::HeaderChecksum(2, 1)
    );
    assert_ne!(
        ProtocolError::WrongCluster(1, 2),
        ProtocolError::WrongCluster(2, 1)
    );
    assert_ne!(
        ProtocolError::ChunkTable(1, 2),
        ProtocolError::ChunkTable(2, 1)
    );
    assert_ne!(
        ProtocolError::FrameTooLarge(1, 2),
        ProtocolError::FrameTooLarge(2, 1)
    );
    assert_ne!(ProtocolError::Chunk(1, 2, 3), ProtocolError::Chunk(1, 3, 2));
}

#[test]
fn test_errors_compare_clone_and_hash_by_value() {
    use std::collections::HashSet;
    let a = ProtocolError::Chunk(0, 0, 100);
    let set: HashSet<ProtocolError> = [a.clone(), ProtocolError::Chunk(0, 0, 100)].into();
    assert_eq!(set.len(), 1);
    assert_eq!(a.clone(), a);
}
