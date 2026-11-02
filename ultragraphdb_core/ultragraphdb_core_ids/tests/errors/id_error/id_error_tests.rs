/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{IdError, IdErrorEnum};

/// Every constructor with the variant it builds, each field given a value distinct from the others'.
fn cases() -> [(IdError, IdErrorEnum); 9] {
    [
        (
            IdError::length("a system ID", 16, 15),
            IdErrorEnum::Length {
                what: "a system ID",
                expected: 16,
                found: 15,
            },
        ),
        (
            IdError::reserved_bits(0x80_0001),
            IdErrorEnum::ReservedBits { bits: 0x80_0001 },
        ),
        (IdError::zero_type_prefix(), IdErrorEnum::ZeroTypePrefix),
        (IdError::zero_sequence(), IdErrorEnum::ZeroSequence),
        (
            IdError::bucket_range(16_777_216),
            IdErrorEnum::BucketRange { bucket: 16_777_216 },
        ),
        (
            IdError::iri_too_long(4_294_967_296),
            IdErrorEnum::IriTooLong { len: 4_294_967_296 },
        ),
        (
            IdError::epoch_range(4_398_046_511_104),
            IdErrorEnum::EpochRange {
                epoch: 4_398_046_511_104,
            },
        ),
        (
            IdError::commit_sequence_range(4_194_304),
            IdErrorEnum::CommitSequenceRange {
                sequence: 4_194_304,
            },
        ),
        (
            IdError::odd_commit_sequence(3),
            IdErrorEnum::OddCommitSequence { sequence: 3 },
        ),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant() {
    for (error, variant) in cases() {
        assert_eq!(error.0, variant);
    }
}

#[test]
fn test_variants_are_distinct() {
    let all = cases();
    for (i, (a, _)) in all.iter().enumerate() {
        for (b, _) in all.iter().skip(i + 1) {
            assert_ne!(a, b);
        }
    }
}
