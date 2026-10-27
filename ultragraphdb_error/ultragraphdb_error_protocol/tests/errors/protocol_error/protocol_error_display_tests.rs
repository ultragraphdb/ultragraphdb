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
        ProtocolError::ShortHeader(0).to_string(),
        "frame of 0 bytes is shorter than its 256-byte header"
    );
    assert_eq!(
        ProtocolError::ShortHeader(255).to_string(),
        "frame of 255 bytes is shorter than its 256-byte header"
    );
    assert_eq!(
        ProtocolError::LengthMismatch(356, 355).to_string(),
        "frame size 356 does not match the 355 bytes received"
    );
    assert_eq!(
        ProtocolError::SizeBelowHeader(255).to_string(),
        "frame size 255 is below its 256-byte header"
    );
    assert_eq!(
        ProtocolError::FrameTooLarge(1_048_577, 1_048_576).to_string(),
        "frame of 1048577 bytes exceeds the 1048576-byte limit"
    );
    assert_eq!(
        ProtocolError::BodyLength(3_841).to_string(),
        "body of 3841 bytes has no split into chunks and their table"
    );
}

#[test]
fn test_checksums_print_as_sixteen_hex_digits() {
    assert_eq!(
        ProtocolError::HeaderChecksum(0x8B60_6B87_756C_942F, 0x1).to_string(),
        "header checksum 0x8b606b87756c942f does not match the computed 0x0000000000000001"
    );
    assert_eq!(
        ProtocolError::ChunkTable(0xB5B8_0037_54D2_87ED, u64::MAX).to_string(),
        "chunk table checksum 0xffffffffffffffff does not match checksum_body 0xb5b8003754d287ed"
    );
}

#[test]
fn test_command_and_reserved_rules_name_the_number_and_offset() {
    assert_eq!(
        ProtocolError::UnknownCommand(0).to_string(),
        "unknown command 0"
    );
    assert_eq!(
        ProtocolError::UnknownCommand(u16::MAX).to_string(),
        "unknown command 65535"
    );
    assert_eq!(
        ProtocolError::ReservedNotZero(127).to_string(),
        "reserved header byte 127 is not zero"
    );
}

#[test]
fn test_wrong_cluster_names_both_clusters() {
    assert_eq!(
        ProtocolError::WrongCluster(0x0123_4567_89AB_CDEF, 0).to_string(),
        "frame of cluster 0x0000000000000000, not this cluster 0x0123456789abcdef"
    );
}

#[test]
fn test_chunk_names_its_index_and_byte_range() {
    assert_eq!(
        ProtocolError::Chunk(3, 12_032, 16_128).to_string(),
        "chunk 3, body bytes 12032..16128, does not match its leaf"
    );
    assert_eq!(
        ProtocolError::Chunk(0, 0, 0).to_string(),
        "chunk 0, body bytes 0..0, does not match its leaf"
    );
}
