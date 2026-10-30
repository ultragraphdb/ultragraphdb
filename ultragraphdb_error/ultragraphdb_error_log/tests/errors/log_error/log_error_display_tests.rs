/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::ErrorKind;

use ultragraphdb_error_log::LogError;

// No Dgraph text applies: the log is the repository's own. Each text names the rule and the values it failed on;
// checksums print as 0x and sixteen hex digits, positions, segments and offsets in decimal.

#[test]
fn test_retry_and_redirect_texts() {
    assert_eq!(
        LogError::NotPrimary().to_string(),
        "this replica is not the primary"
    );
    assert_eq!(
        LogError::Busy(64).to_string(),
        "the log holds its capacity of 64 segments; a checkpoint must trim it first"
    );
    assert_eq!(
        LogError::Sealed(9).to_string(),
        "the log was sealed at position 9"
    );
}

#[test]
fn test_outcome_unknown_text() {
    assert_eq!(
        LogError::Truncated(5).to_string(),
        "round 5 was replaced before it became durable; its outcome is unknown"
    );
}

#[test]
fn test_programming_error_texts() {
    assert_eq!(
        LogError::PositionMismatch(6, 7).to_string(),
        "position 7 is not the expected 6"
    );
    assert_eq!(
        LogError::ParentMismatch(6, 0x8B60_6B87_756C_942F, 1).to_string(),
        "round 6 names parent 0x0000000000000001, not the previous round's checksum 0x8b606b87756c942f"
    );
    assert_eq!(
        LogError::TooLarge(1_048_577, 1_048_576).to_string(),
        "round of 1048577 bytes exceeds the 1048576-byte limit"
    );
}

#[test]
fn test_fatal_texts_name_where() {
    assert_eq!(
        LogError::Corrupt(2, 0, 4_096).to_string(),
        "round 2 at segment 0 byte 4096 is damaged, and a complete round follows it"
    );
    assert_eq!(
        LogError::CorruptChunk(2, 1, 8_192, 3, 12_032, 16_128).to_string(),
        "chunk 3, body bytes 12032..16128, of round 2 at segment 1 byte 8192 is damaged, and a complete round follows it"
    );
    assert_eq!(
        LogError::SuperblockUnreadable('a').to_string(),
        "superblock copy a does not verify; the log is not started afresh over it"
    );
    assert_eq!(
        LogError::Diverged("the checkpoint's checksum", 7, 8).to_string(),
        "the log diverged: the checkpoint's checksum is 8, not 7"
    );
    assert_eq!(
        LogError::Io("file sync", ErrorKind::Other, "injected").to_string(),
        "storage file sync failed (other error): injected"
    );
    assert_eq!(
        LogError::Poisoned().to_string(),
        "the log is poisoned by an earlier failed write or sync; open it again"
    );
}

#[test]
fn test_exhausted_text_names_the_last_position() {
    // u64::MAX, the last position a round can take.
    assert_eq!(
        LogError::Exhausted(18_446_744_073_709_551_615).to_string(),
        "the log holds round 18446744073709551615, the last position; no round can follow it"
    );
}
