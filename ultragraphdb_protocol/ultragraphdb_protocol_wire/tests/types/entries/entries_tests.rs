/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Entries, EntriesEnd};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

#[test]
fn test_the_golden_entries_are_built_byte_for_byte() -> TestResult {
    let golden = golden_role("entries")?;
    let mut entries = Entries::new(2, EntriesEnd::Ended, 0x1234);
    entries.push(b"\x10key-a", b"value-a");
    entries.push(b"\x10key-b", b"");
    entries.push(b"\x10key-b\x00", b"v");
    assert_eq!(entries.entries().len(), 3);
    assert_eq!(entries.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

#[test]
fn test_a_batch_of_no_entries_has_an_empty_section() {
    let entries = Entries::new(0, EntriesEnd::More, 0);
    let header = entries.command_header();
    assert_eq!(header, [0; 128]);
    // One section of length 0, at offset 8.
    assert_eq!(entries.body(), [8, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn test_the_command_header_holds_count_range_end_and_epoch() {
    let mut entries = Entries::new(0x0102_0304, EntriesEnd::Limit, 0x0000_0302_0100_0F0E);
    entries.push(b"k", b"v");
    let header = entries.command_header();
    assert_eq!(&header[0..4], &1u32.to_le_bytes());
    assert_eq!(&header[4..8], &0x0102_0304u32.to_le_bytes());
    assert_eq!(header[8], 2);
    assert_eq!(&header[9..16], &[0; 7]);
    assert_eq!(&header[16..24], &0x0000_0302_0100_0F0Eu64.to_le_bytes());
    assert!(header[24..].iter().all(|b| *b == 0));
}

#[test]
fn test_the_getters_give_back_what_the_constructor_and_pushes_took() {
    let mut entries = Entries::new(0x0102_0304, EntriesEnd::Limit, u64::MAX);
    assert!(entries.entries().is_empty());
    entries.push(b"k1", b"");
    entries.push(b"k2", b"v2");
    assert_eq!(
        (entries.range(), entries.end(), entries.epoch()),
        (0x0102_0304, EntriesEnd::Limit, u64::MAX)
    );
    assert_eq!(
        entries.entries(),
        [
            (b"k1".to_vec(), Vec::new()),
            (b"k2".to_vec(), b"v2".to_vec())
        ]
        .as_slice()
    );
    for end in [EntriesEnd::More, EntriesEnd::Ended, EntriesEnd::Limit] {
        assert_eq!(Entries::new(0, end, 0).end(), end);
    }
}
