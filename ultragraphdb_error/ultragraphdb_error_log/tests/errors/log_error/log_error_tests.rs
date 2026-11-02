/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::ErrorKind;

use ultragraphdb_error_log::{LogError, LogErrorEnum};

/// The constructors of the log's state and its positions, each with the variant it builds.
fn position_cases() -> [(LogError, LogErrorEnum); 8] {
    [
        (LogError::not_primary(), LogErrorEnum::NotPrimary),
        (LogError::busy(8), LogErrorEnum::Busy { segments: 8 }),
        (LogError::sealed(9), LogErrorEnum::Sealed { position: 9 }),
        (
            LogError::exhausted(11),
            LogErrorEnum::Exhausted { position: 11 },
        ),
        (
            LogError::truncated(5),
            LogErrorEnum::Truncated { position: 5 },
        ),
        (
            LogError::position_mismatch(6, 7),
            LogErrorEnum::PositionMismatch {
                expected: 6,
                found: 7,
            },
        ),
        (
            LogError::parent_mismatch(6, 0xA1, 0xB2),
            LogErrorEnum::ParentMismatch {
                position: 6,
                expected: 0xA1,
                found: 0xB2,
            },
        ),
        (
            LogError::too_large(1_048_577, 1_048_576),
            LogErrorEnum::TooLarge {
                size: 1_048_577,
                limit: 1_048_576,
            },
        ),
    ]
}

/// The constructors of corruption, divergence and failure, each with the variant it builds.
fn fault_cases() -> [(LogError, LogErrorEnum); 6] {
    [
        (
            LogError::corrupt(2, 3, 4_096),
            LogErrorEnum::Corrupt {
                position: 2,
                segment: 3,
                offset: 4_096,
            },
        ),
        (
            LogError::corrupt_chunk(2, 3, 4_096, 5, 12_032, 16_128),
            LogErrorEnum::CorruptChunk {
                position: 2,
                segment: 3,
                offset: 4_096,
                chunk: 5,
                start: 12_032,
                end: 16_128,
            },
        ),
        (
            LogError::superblock_unreadable('b'),
            LogErrorEnum::SuperblockUnreadable { copy: 'b' },
        ),
        (
            LogError::diverged("cluster", 1, 2),
            LogErrorEnum::Diverged {
                what: "cluster",
                expected: 1,
                found: 2,
            },
        ),
        (
            LogError::io("write", ErrorKind::UnexpectedEof, "short"),
            LogErrorEnum::Io {
                operation: "write",
                kind: ErrorKind::UnexpectedEof,
                message: "short".to_string(),
            },
        ),
        (LogError::poisoned(), LogErrorEnum::Poisoned),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant_with_its_values() {
    // Every value distinct, so a constructor that swaps two fields is caught.
    for (built, expected) in position_cases().into_iter().chain(fault_cases()) {
        assert_eq!(built.0, expected);
    }
}

#[test]
fn test_errors_compare_by_variant_and_values() {
    assert_eq!(LogError::sealed(9), LogError::sealed(9));
    assert_ne!(LogError::sealed(9), LogError::sealed(10));
    assert_ne!(LogError::sealed(9), LogError::truncated(9));
    assert_ne!(LogError::sealed(9), LogError::exhausted(9));
    assert_eq!(LogError::poisoned().clone(), LogError::poisoned());
}
