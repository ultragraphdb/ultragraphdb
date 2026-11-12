/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, CHUNK_SIZE, Command, Entries, EntriesEnd, FIRST_CHUNK_SIZE, Header,
    ObjectRecordsBuilder,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// An object of the golden lists: its encoding and the statement IDs it is held under.
pub type Object = (Vec<u8>, Vec<[u8; 16]>);

/// The key of the list the golden frames hold whole.
pub const LIST_A: &[u8] = b"\x10list-a";
/// The key of the list the golden frames hold across two frames.
pub const LIST_B: &[u8] = b"\x10list-b";
/// The epoch the golden lists were read at.
pub const EPOCH: u64 = 0x1234;
/// The version the golden lists were read at: the epoch's version bound.
pub const VERSION: u64 = (EPOCH << 22) | 0x3F_FFFF;

/// A system ID's 16 big-endian bytes.
fn id(prefix: u16, sequence: u64, bucket: u32) -> [u8; 16] {
    let mut out = [0; 16];
    out[..2].copy_from_slice(&prefix.to_be_bytes());
    out[2..10].copy_from_slice(&sequence.to_be_bytes());
    out[10..13].copy_from_slice(&bucket.to_be_bytes()[1..]);
    out
}

/// The objects of [`LIST_A`].
pub fn list_a() -> Vec<Object> {
    vec![(id(1, 21, 0x00_0A01).to_vec(), Vec::new())]
}

/// The objects of [`LIST_B`] in the first frame.
pub fn list_b_first() -> Vec<Object> {
    vec![(
        id(1, 22, 0x00_0A02).to_vec(),
        vec![id(2, 31, 0x00_0B01), id(2, 32, 0x00_0B02)],
    )]
}

/// The objects of [`LIST_B`] in the second frame.
pub fn list_b_rest() -> Vec<Object> {
    vec![
        (b"\x30abc".to_vec(), Vec::new()),
        (id(1, 23, 0x00_0A03).to_vec(), vec![id(2, 33, 0x00_0B03)]),
    ]
}

/// The value holding `objects`.
pub fn value_of(objects: &[Object]) -> Vec<u8> {
    let mut value = ObjectRecordsBuilder::new();
    objects
        .iter()
        .for_each(|(encoding, statements)| value.push(encoding, statements));
    value.value().to_vec()
}

/// The golden frame ending between two object records: range 0, [`LIST_A`] whole and [`LIST_B`]'s first object.
pub fn golden_entries_continues() -> Entries {
    let mut entries = Entries::continuing(0, EPOCH).with_version(VERSION);
    entries.push(LIST_A, &value_of(&list_a()));
    entries.push(LIST_B, &value_of(&list_b_first()));
    entries
}

/// The golden frame after it: [`LIST_B`] again with its next objects, ending the range.
pub fn golden_entries_continued() -> Entries {
    let mut entries = Entries::new(0, EntriesEnd::Ended, EPOCH).with_version(VERSION);
    entries.push(LIST_B, &value_of(&list_b_rest()));
    entries
}

#[test]
fn test_the_golden_frames_across_a_list_are_built_byte_for_byte() -> TestResult {
    for (name, built) in [
        ("entries_continues", golden_entries_continues()),
        ("entries_continued", golden_entries_continued()),
    ] {
        let golden = golden_role(name)?;
        assert_eq!(built.frame(golden.header())?, role_bytes(&golden)?);
    }
    Ok(())
}

#[test]
fn test_a_continuing_batch_has_end_0_and_says_so_at_byte_9() {
    let entries = Entries::continuing(7, EPOCH);
    assert!(entries.continues());
    assert_eq!(entries.end(), EntriesEnd::More);
    assert_eq!((entries.range(), entries.epoch()), (7, EPOCH));
    let header = entries.command_header();
    assert_eq!(header[8], EntriesEnd::More.number());
    assert_eq!(header[9], 1);
}

#[test]
fn test_a_batch_continues_only_when_built_to() {
    for end in [EntriesEnd::More, EntriesEnd::Ended, EntriesEnd::Limit] {
        let entries = Entries::new(0, end, 0);
        assert!(!entries.continues());
        assert_eq!(entries.command_header()[9], 0);
    }
}

#[test]
fn test_the_golden_entries_are_built_byte_for_byte() -> TestResult {
    let golden = golden_role("entries")?;
    let mut entries = Entries::new(2, EntriesEnd::Ended, EPOCH).with_version(VERSION);
    entries.push(b"\x10key-a", b"value-a");
    entries.push(b"\x10key-b", b"");
    entries.push(b"\x10key-b\x00", b"v");
    assert_eq!(entries.entries().len(), 3);
    assert_eq!(entries.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

/// The frame sizes the batch states against the frame it builds.
fn assert_sizes(lengths: &[(usize, usize)]) -> TestResult {
    let mut entries = Entries::new(0, EntriesEnd::More, 0);
    let frame_len = |e: &Entries| e.frame(Header::new(1, Command::Entries)).map(|f| f.len());
    assert_eq!(entries.frame_size(), frame_len(&entries)?);
    for (at, (key_len, value_len)) in lengths.iter().enumerate() {
        let promised = entries.frame_size_with(*key_len, *value_len);
        // Keys ascend by their first byte, whatever their lengths.
        let mut key = vec![0; *key_len];
        key[0] = u8::try_from(at)?;
        entries.push(&key, &vec![0xAB; *value_len]);
        assert_eq!(promised, frame_len(&entries)?, "{key_len} {value_len}");
        assert_eq!(entries.frame_size(), promised, "{key_len} {value_len}");
    }
    Ok(())
}

#[test]
fn test_the_stated_sizes_hold_the_padding_of_every_length() -> TestResult {
    for key_len in 1..=9 {
        for value_len in 0..=9 {
            assert_sizes(&[(key_len, value_len), (key_len, value_len)])?;
        }
    }
    Ok(())
}

/// A body of 3,840 bytes is one chunk with no table, one past it two chunks and a table of 16 bytes, and each 4,096 bytes
/// more a chunk more.
#[test]
fn test_the_stated_sizes_hold_the_chunk_table() -> TestResult {
    let first = FIRST_CHUNK_SIZE;
    let body_before = 8 + 8 + 1;
    for chunk_end in [first, first + CHUNK_SIZE, first + 2 * CHUNK_SIZE] {
        for value_len in chunk_end - body_before - 16..=chunk_end - body_before + 16 {
            assert_sizes(&[(1, value_len)])?;
        }
    }
    assert_sizes(&[(1, first), (2, CHUNK_SIZE), (3, 2 * CHUNK_SIZE)])
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
fn test_the_command_header_holds_count_range_end_through_epoch_and_version() {
    let mut entries = Entries::new(0x0102_0304, EntriesEnd::Limit, 0x0000_0302_0100_0F0E)
        .with_through(0x0506_0708)
        .with_version(VERSION);
    entries.push(b"k", b"v");
    let header = entries.command_header();
    assert_eq!(&header[0..4], &1u32.to_le_bytes());
    assert_eq!(&header[4..8], &0x0102_0304u32.to_le_bytes());
    assert_eq!(header[8], 2);
    assert_eq!(&header[9..12], &[0; 3]);
    assert_eq!(&header[12..16], &0x0506_0708u32.to_le_bytes());
    assert_eq!(&header[16..24], &0x0000_0302_0100_0F0Eu64.to_le_bytes());
    assert_eq!(&header[24..32], &VERSION.to_le_bytes());
    assert!(header[32..].iter().all(|b| *b == 0));
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

/// A batch filled before how it ends is known says so afterwards, and its frame is the one built knowing it.
#[test]
fn test_a_batch_takes_its_end_once_filled() {
    let mut filled = Entries::new(3, EntriesEnd::More, EPOCH);
    filled.push(LIST_A, b"value");
    let ended = filled.clone().with_end(EntriesEnd::Limit);
    assert_eq!(ended.end(), EntriesEnd::Limit);
    let mut known = Entries::new(3, EntriesEnd::Limit, EPOCH);
    known.push(LIST_A, b"value");
    assert_eq!(ended, known);
    let continued = filled.continued();
    assert!(continued.continues());
    assert_eq!(continued.end(), EntriesEnd::More);
    let mut continuing = Entries::continuing(3, EPOCH);
    continuing.push(LIST_A, b"value");
    assert_eq!(continued, continuing);
}

#[test]
fn test_a_batch_names_version_0_until_given_its_scans() {
    let entries = Entries::new(1, EntriesEnd::More, EPOCH);
    assert_eq!(entries.version(), 0);
    assert_eq!(entries.with_version(VERSION).version(), VERSION);
    assert_eq!(
        Entries::continuing(1, EPOCH)
            .with_version(u64::MAX)
            .version(),
        u64::MAX
    );
}
