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
        (
            ProtocolError::WrongCommand(1, 2),
            ProtocolErrorEnum::WrongCommand {
                expected: 1,
                found: 2,
            },
        ),
        (
            ProtocolError::BodyTooShort(7, 3, 32),
            ProtocolErrorEnum::BodyTooShort {
                command: 7,
                len: 3,
                need: 32,
            },
        ),
        (
            ProtocolError::SectionOutOfBounds(7, 2, 64, 8, 60),
            ProtocolErrorEnum::SectionOutOfBounds {
                command: 7,
                section: 2,
                offset: 64,
                len: 8,
                body: 60,
            },
        ),
        (
            ProtocolError::SectionOffset(7, 1, 33, 40),
            ProtocolErrorEnum::SectionOffset {
                command: 7,
                section: 1,
                offset: 33,
                expected: 40,
            },
        ),
        (
            ProtocolError::PaddingNotZero(5, 41),
            ProtocolErrorEnum::PaddingNotZero {
                command: 5,
                offset: 41,
            },
        ),
        (
            ProtocolError::TrailingBytes(5, 40, 41),
            ProtocolErrorEnum::TrailingBytes {
                command: 5,
                end: 40,
                len: 41,
            },
        ),
        (
            ProtocolError::SectionLength(5, 0, 31, 16),
            ProtocolErrorEnum::SectionLength {
                command: 5,
                section: 0,
                len: 31,
                unit: 16,
            },
        ),
        (
            ProtocolError::EntryOutOfBounds(17, 1, 2, 32),
            ProtocolErrorEnum::EntryOutOfBounds {
                command: 17,
                section: 1,
                entry: 2,
                offset: 32,
            },
        ),
        (
            ProtocolError::CountMismatch(13, "cells", 5, 6),
            ProtocolErrorEnum::CountMismatch {
                command: 13,
                field: "cells",
                count: 5,
                expected: 6,
            },
        ),
        (
            ProtocolError::NotUtf8(1, 0, 3),
            ProtocolErrorEnum::NotUtf8 {
                command: 1,
                section: 0,
                offset: 3,
            },
        ),
        (
            ProtocolError::IndexOutOfRange(13, 0, 2, 4, 3),
            ProtocolErrorEnum::IndexOutOfRange {
                command: 13,
                section: 0,
                at: 2,
                index: 4,
                limit: 3,
            },
        ),
        (
            ProtocolError::BadTerm(8, 3, 9),
            ProtocolErrorEnum::BadTerm {
                command: 8,
                section: 3,
                at: 9,
            },
        ),
        (
            ProtocolError::FieldValue(4, "mode", 2),
            ProtocolErrorEnum::FieldValue {
                command: 4,
                field: "mode",
                value: 2,
            },
        ),
        (
            ProtocolError::DuplicateTerm(8, 2, 5, 3),
            ProtocolErrorEnum::DuplicateTerm {
                command: 8,
                section: 2,
                at: 5,
                earlier: 3,
            },
        ),
        (
            ProtocolError::TermTooDeep(11, 0, 33, 33, 32),
            ProtocolErrorEnum::TermTooDeep {
                command: 11,
                section: 0,
                at: 33,
                depth: 33,
                limit: 32,
            },
        ),
        (
            ProtocolError::Record(1, ProtocolError::PaddingNotZero(17, 9)),
            ProtocolErrorEnum::Record {
                index: 1,
                cause: Box::new(ProtocolErrorEnum::PaddingNotZero {
                    command: 17,
                    offset: 9,
                }),
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
    assert_ne!(
        ProtocolError::WrongCommand(1, 2),
        ProtocolError::WrongCommand(2, 1)
    );
    assert_ne!(
        ProtocolError::BodyTooShort(1, 2, 3),
        ProtocolError::BodyTooShort(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::SectionOutOfBounds(1, 2, 3, 4, 5),
        ProtocolError::SectionOutOfBounds(1, 2, 4, 3, 5)
    );
    assert_ne!(
        ProtocolError::SectionOffset(1, 2, 3, 4),
        ProtocolError::SectionOffset(1, 2, 4, 3)
    );
    assert_ne!(
        ProtocolError::TrailingBytes(1, 2, 3),
        ProtocolError::TrailingBytes(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::SectionLength(1, 2, 3, 4),
        ProtocolError::SectionLength(1, 2, 4, 3)
    );
    assert_ne!(
        ProtocolError::EntryOutOfBounds(1, 2, 3, 4),
        ProtocolError::EntryOutOfBounds(1, 2, 4, 3)
    );
    assert_ne!(
        ProtocolError::CountMismatch(1, "f", 2, 3),
        ProtocolError::CountMismatch(1, "f", 3, 2)
    );
    assert_ne!(
        ProtocolError::NotUtf8(1, 2, 3),
        ProtocolError::NotUtf8(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::IndexOutOfRange(1, 2, 3, 4, 5),
        ProtocolError::IndexOutOfRange(1, 2, 3, 5, 4)
    );
    assert_ne!(
        ProtocolError::BadTerm(1, 2, 3),
        ProtocolError::BadTerm(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::FieldValue(1, "f", 2),
        ProtocolError::FieldValue(2, "f", 1)
    );
}

#[test]
fn test_errors_compare_clone_and_hash_by_value() {
    use std::collections::HashSet;
    let a = ProtocolError::Chunk(0, 0, 100);
    let set: HashSet<ProtocolError> = [a.clone(), ProtocolError::Chunk(0, 0, 100)].into();
    assert_eq!(set.len(), 1);
    assert_eq!(a.clone(), a);
}
