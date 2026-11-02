/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

// No Dgraph text applies: the protocol is the repository's own. Each text names the rule and the values it failed on;
// checksums and cluster identities print as 0x and sixteen hex digits.

#[test]
fn test_length_rules_name_the_lengths() {
    assert_eq!(
        ProtocolError::short_header(0).to_string(),
        "frame of 0 bytes is shorter than its 256-byte header"
    );
    assert_eq!(
        ProtocolError::short_header(255).to_string(),
        "frame of 255 bytes is shorter than its 256-byte header"
    );
    assert_eq!(
        ProtocolError::length_mismatch(356, 355).to_string(),
        "frame size 356 does not match the 355 bytes received"
    );
    assert_eq!(
        ProtocolError::size_below_header(255).to_string(),
        "frame size 255 is below its 256-byte header"
    );
    assert_eq!(
        ProtocolError::frame_too_large(1_048_577, 1_048_576).to_string(),
        "frame of 1048577 bytes exceeds the 1048576-byte limit"
    );
    assert_eq!(
        ProtocolError::body_length(3_841).to_string(),
        "body of 3841 bytes has no split into chunks and their table"
    );
}

#[test]
fn test_checksums_print_as_sixteen_hex_digits() {
    assert_eq!(
        ProtocolError::header_checksum(0x8B60_6B87_756C_942F, 0x1).to_string(),
        "header checksum 0x8b606b87756c942f does not match the computed 0x0000000000000001"
    );
    assert_eq!(
        ProtocolError::chunk_table(0xB5B8_0037_54D2_87ED, u64::MAX).to_string(),
        "chunk table checksum 0xffffffffffffffff does not match checksum_body 0xb5b8003754d287ed"
    );
}

#[test]
fn test_command_and_reserved_rules_name_the_number_and_offset() {
    assert_eq!(
        ProtocolError::unknown_command(0).to_string(),
        "unknown command 0"
    );
    assert_eq!(
        ProtocolError::unknown_command(u16::MAX).to_string(),
        "unknown command 65535"
    );
    assert_eq!(
        ProtocolError::reserved_not_zero(127).to_string(),
        "reserved header byte 127 is not zero"
    );
}

#[test]
fn test_wrong_cluster_names_both_clusters() {
    assert_eq!(
        ProtocolError::wrong_cluster(0x0123_4567_89AB_CDEF, 0).to_string(),
        "frame of cluster 0x0000000000000000, not this cluster 0x0123456789abcdef"
    );
}

#[test]
fn test_chunk_names_its_index_and_byte_range() {
    assert_eq!(
        ProtocolError::chunk(3, 12_032, 16_128).to_string(),
        "chunk 3, body bytes 12032..16128, does not match its leaf"
    );
    assert_eq!(
        ProtocolError::chunk(0, 0, 0).to_string(),
        "chunk 0, body bytes 0..0, does not match its leaf"
    );
    assert_eq!(
        ProtocolError::sequence_order(7, 9).to_string(),
        "frame of sequence 9 where sequence 7 was due"
    );
}

#[test]
fn test_body_rules_name_the_command_section_and_values() {
    assert_eq!(
        ProtocolError::wrong_command(1, 2).to_string(),
        "a view of command 1 was asked of a frame of command 2"
    );
    assert_eq!(
        ProtocolError::body_too_short(7, 3, 32).to_string(),
        "body of command 7, 3 bytes, is shorter than its 32-byte fixed part and section table"
    );
    assert_eq!(
        ProtocolError::section_out_of_bounds(7, 2, 64, 8, 60).to_string(),
        "section 2 of command 7, 8 bytes at 64, runs past the 60-byte body"
    );
    assert_eq!(
        ProtocolError::section_offset(7, 1, 33, 40).to_string(),
        "section 1 of command 7 starts at 33, not at 40"
    );
    assert_eq!(
        ProtocolError::padding_not_zero(5, 41).to_string(),
        "padding byte 41 of a body of command 5 is not zero"
    );
    assert_eq!(
        ProtocolError::trailing_bytes(5, 40, 41).to_string(),
        "body of command 5 holds 41 bytes, past the end of its last section at 40"
    );
    assert_eq!(
        ProtocolError::section_length(5, 0, 31, 16).to_string(),
        "section 0 of command 5, 31 bytes, is not a whole number of 16-byte records"
    );
    assert_eq!(
        ProtocolError::entry_out_of_bounds(17, 1, 2, 32).to_string(),
        "entry 2 of section 1 of command 17, at 32, runs past its section"
    );
    assert_eq!(
        ProtocolError::count_mismatch(13, "cells", 5, 6).to_string(),
        "cells of command 13 counts 5 where the body holds 6"
    );
    assert_eq!(
        ProtocolError::not_utf8(1, 0, 3).to_string(),
        "section 0 of command 1 is not UTF-8 at 3"
    );
    assert_eq!(
        ProtocolError::index_out_of_range(13, 0, 2, 4, 3).to_string(),
        "entry 2 of section 0 of command 13 is 4, past the 3 it indexes"
    );
    assert_eq!(
        ProtocolError::bad_term(8, 3, 9).to_string(),
        "term 9 of section 3 of command 8 is not a term of its place"
    );
    assert_eq!(
        ProtocolError::field_value(4, "mode", 2).to_string(),
        "mode of command 4 holds 2, which the command does not define"
    );
    assert_eq!(
        ProtocolError::duplicate_term(8, 2, 5, 3).to_string(),
        "entry 5 of the term table in section 2 of command 8 repeats entry 3"
    );
    assert_eq!(
        ProtocolError::term_too_deep(11, 0, 33, 33, 32).to_string(),
        "entry 33 of the term table in section 0 of command 11 nests 33 deep, past the 32 allowed"
    );
    assert_eq!(
        ProtocolError::record(1, ProtocolError::padding_not_zero(17, 9)).to_string(),
        "write-set record 1 of the round: padding byte 9 of a body of command 17 is not zero"
    );
}
