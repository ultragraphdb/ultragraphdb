/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};

/// The constructors of the frame's header, size and checksums, each with the variant it builds.
fn header_cases() -> [(ProtocolError, ProtocolErrorEnum); 11] {
    [
        (
            ProtocolError::short_header(255),
            ProtocolErrorEnum::ShortHeader { len: 255 },
        ),
        (
            ProtocolError::header_checksum(1, 2),
            ProtocolErrorEnum::HeaderChecksum {
                stored: 1,
                computed: 2,
            },
        ),
        (
            ProtocolError::unknown_command(18),
            ProtocolErrorEnum::UnknownCommand { command: 18 },
        ),
        (
            ProtocolError::command_not_served(512),
            ProtocolErrorEnum::CommandNotServed { command: 512 },
        ),
        (
            ProtocolError::reserved_not_zero(66),
            ProtocolErrorEnum::ReservedNotZero { offset: 66 },
        ),
        (
            ProtocolError::wrong_cluster(3, 4),
            ProtocolErrorEnum::WrongCluster {
                expected: 3,
                found: 4,
            },
        ),
        (
            ProtocolError::size_below_header(255),
            ProtocolErrorEnum::SizeBelowHeader { size: 255 },
        ),
        (
            ProtocolError::frame_too_large(1_048_577, 1_048_576),
            ProtocolErrorEnum::FrameTooLarge {
                size: 1_048_577,
                limit: 1_048_576,
            },
        ),
        (
            ProtocolError::length_mismatch(356, 355),
            ProtocolErrorEnum::LengthMismatch {
                size: 356,
                len: 355,
            },
        ),
        (
            ProtocolError::body_length(3_841),
            ProtocolErrorEnum::BodyLength { len: 3_841 },
        ),
        (
            ProtocolError::chunk_table(5, 6),
            ProtocolErrorEnum::ChunkTable {
                stored: 5,
                computed: 6,
            },
        ),
    ]
}

/// The constructors of a chunk, the frame's order and command, and a body's layout, each with the variant it builds.
fn frame_cases() -> [(ProtocolError, ProtocolErrorEnum); 7] {
    [
        (
            ProtocolError::chunk(3, 12_032, 16_128),
            ProtocolErrorEnum::Chunk {
                index: 3,
                start: 12_032,
                end: 16_128,
            },
        ),
        (
            ProtocolError::sequence_order(2, 4),
            ProtocolErrorEnum::SequenceOrder {
                expected: 2,
                found: 4,
            },
        ),
        (
            ProtocolError::wrong_command(1, 2),
            ProtocolErrorEnum::WrongCommand {
                expected: 1,
                found: 2,
            },
        ),
        (
            ProtocolError::body_too_short(7, 3, 32),
            ProtocolErrorEnum::BodyTooShort {
                command: 7,
                len: 3,
                need: 32,
            },
        ),
        (
            ProtocolError::section_out_of_bounds(7, 2, 64, 8, 60),
            ProtocolErrorEnum::SectionOutOfBounds {
                command: 7,
                section: 2,
                offset: 64,
                len: 8,
                body: 60,
            },
        ),
        (
            ProtocolError::section_offset(7, 1, 33, 40),
            ProtocolErrorEnum::SectionOffset {
                command: 7,
                section: 1,
                offset: 33,
                expected: 40,
            },
        ),
        (
            ProtocolError::padding_not_zero(5, 41),
            ProtocolErrorEnum::PaddingNotZero {
                command: 5,
                offset: 41,
            },
        ),
    ]
}

/// The constructors of a body's sections, entries, counts and text, each with the variant it builds.
fn section_cases() -> [(ProtocolError, ProtocolErrorEnum); 6] {
    [
        (
            ProtocolError::trailing_bytes(5, 40, 41),
            ProtocolErrorEnum::TrailingBytes {
                command: 5,
                end: 40,
                len: 41,
            },
        ),
        (
            ProtocolError::section_length(5, 0, 31, 16),
            ProtocolErrorEnum::SectionLength {
                command: 5,
                section: 0,
                len: 31,
                unit: 16,
            },
        ),
        (
            ProtocolError::entry_out_of_bounds(17, 1, 2, 32),
            ProtocolErrorEnum::EntryOutOfBounds {
                command: 17,
                section: 1,
                entry: 2,
                offset: 32,
            },
        ),
        (
            ProtocolError::count_mismatch(13, "cells", 5, 6),
            ProtocolErrorEnum::CountMismatch {
                command: 13,
                field: "cells",
                count: 5,
                expected: 6,
            },
        ),
        (
            ProtocolError::not_utf8(1, 0, 3),
            ProtocolErrorEnum::NotUtf8 {
                command: 1,
                section: 0,
                offset: 3,
            },
        ),
        (
            ProtocolError::index_out_of_range(13, 0, 2, 4, 3),
            ProtocolErrorEnum::IndexOutOfRange {
                command: 13,
                section: 0,
                at: 2,
                index: 4,
                limit: 3,
            },
        ),
    ]
}

/// The constructors of terms, field values and a round's records, each with the variant it builds.
fn content_cases() -> [(ProtocolError, ProtocolErrorEnum); 6] {
    [
        (
            ProtocolError::bad_term(8, 3, 9),
            ProtocolErrorEnum::BadTerm {
                command: 8,
                section: 3,
                at: 9,
            },
        ),
        (
            ProtocolError::field_value(4, "mode", 2),
            ProtocolErrorEnum::FieldValue {
                command: 4,
                field: "mode",
                value: 2,
            },
        ),
        (
            ProtocolError::duplicate_term(8, 2, 5, 3),
            ProtocolErrorEnum::DuplicateTerm {
                command: 8,
                section: 2,
                at: 5,
                earlier: 3,
            },
        ),
        (
            ProtocolError::term_too_deep(11, 0, 33, 33, 32),
            ProtocolErrorEnum::TermTooDeep {
                command: 11,
                section: 0,
                at: 33,
                depth: 33,
                limit: 32,
            },
        ),
        (
            ProtocolError::record(1, ProtocolError::padding_not_zero(17, 9)),
            ProtocolErrorEnum::Record {
                index: 1,
                cause: Box::new(ProtocolErrorEnum::PaddingNotZero {
                    command: 17,
                    offset: 9,
                }),
            },
        ),
        (
            ProtocolError::past_credit(7),
            ProtocolErrorEnum::PastCredit { stream: 7 },
        ),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant_with_its_values() {
    let cases = header_cases()
        .into_iter()
        .chain(frame_cases())
        .chain(section_cases())
        .chain(content_cases());
    for (error, want) in cases {
        assert_eq!(error.0, want);
    }
}

#[test]
fn test_argument_order_is_kept_where_two_values_share_a_type() {
    // Distinct values in each position, so a constructor swapping its arguments fails.
    assert_ne!(
        ProtocolError::header_checksum(1, 2),
        ProtocolError::header_checksum(2, 1)
    );
    assert_ne!(
        ProtocolError::wrong_cluster(1, 2),
        ProtocolError::wrong_cluster(2, 1)
    );
    assert_ne!(
        ProtocolError::chunk_table(1, 2),
        ProtocolError::chunk_table(2, 1)
    );
    assert_ne!(
        ProtocolError::frame_too_large(1, 2),
        ProtocolError::frame_too_large(2, 1)
    );
    assert_ne!(ProtocolError::chunk(1, 2, 3), ProtocolError::chunk(1, 3, 2));
    assert_ne!(
        ProtocolError::sequence_order(1, 2),
        ProtocolError::sequence_order(2, 1)
    );
    assert_ne!(
        ProtocolError::wrong_command(1, 2),
        ProtocolError::wrong_command(2, 1)
    );
    assert_ne!(
        ProtocolError::body_too_short(1, 2, 3),
        ProtocolError::body_too_short(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::section_out_of_bounds(1, 2, 3, 4, 5),
        ProtocolError::section_out_of_bounds(1, 2, 4, 3, 5)
    );
    assert_ne!(
        ProtocolError::section_offset(1, 2, 3, 4),
        ProtocolError::section_offset(1, 2, 4, 3)
    );
    assert_ne!(
        ProtocolError::trailing_bytes(1, 2, 3),
        ProtocolError::trailing_bytes(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::section_length(1, 2, 3, 4),
        ProtocolError::section_length(1, 2, 4, 3)
    );
    assert_ne!(
        ProtocolError::entry_out_of_bounds(1, 2, 3, 4),
        ProtocolError::entry_out_of_bounds(1, 2, 4, 3)
    );
    assert_ne!(
        ProtocolError::count_mismatch(1, "f", 2, 3),
        ProtocolError::count_mismatch(1, "f", 3, 2)
    );
    assert_ne!(
        ProtocolError::not_utf8(1, 2, 3),
        ProtocolError::not_utf8(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::index_out_of_range(1, 2, 3, 4, 5),
        ProtocolError::index_out_of_range(1, 2, 3, 5, 4)
    );
    assert_ne!(
        ProtocolError::bad_term(1, 2, 3),
        ProtocolError::bad_term(1, 3, 2)
    );
    assert_ne!(
        ProtocolError::field_value(1, "f", 2),
        ProtocolError::field_value(2, "f", 1)
    );
}

#[test]
fn test_errors_compare_clone_and_hash_by_value() {
    use std::collections::HashSet;
    let a = ProtocolError::chunk(0, 0, 100);
    let set: HashSet<ProtocolError> = [a.clone(), ProtocolError::chunk(0, 0, 100)].into();
    assert_eq!(set.len(), 1);
    assert_eq!(a.clone(), a);
}
