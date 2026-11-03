/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, EntriesEnd, EntriesView, MAX_EPOCH};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::refusal;

fn golden() -> TestResult<Vec<u8>> {
    role_bytes(&golden_role("entries")?)
}

/// An entry laid out from the key-value table: `key_len` u32, `value_len` u32 (`u32::MAX` for none), the key, the
/// value, zeros to a multiple of 8.
fn entry(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let mut out = (key.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&value.map_or(u32::MAX, |v| v.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(value.unwrap_or_default());
    out.resize(out.len().div_ceil(8) * 8, 0);
    out
}

/// The golden frame with its entries replaced by `entries` and its count by `count`.
fn with_entries(entries: &[Vec<u8>], count: u32) -> TestResult<Vec<u8>> {
    let bytes = with_section(&golden()?, 1, 0, |s| *s = entries.concat());
    Ok(with_command_header(&bytes, |h| put(h, 0, count.into(), 4)))
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 525,
        field,
        value,
    }
}

#[test]
fn test_the_golden_entries_read_in_place() -> TestResult {
    let bytes = golden()?;
    let view = EntriesView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.range(), 2);
    assert_eq!(view.end(), EntriesEnd::Ended);
    assert_eq!(view.epoch(), 0x1234);
    let read: Vec<(&[u8], Option<&[u8]>)> = view
        .entries()
        .iter()
        .map(|e| (e.key(), e.value()))
        .collect();
    assert_eq!(
        read,
        vec![
            (&b"\x10key-a"[..], Some(&b"value-a"[..])),
            (&b"\x10key-b"[..], Some(&b""[..])),
            (&b"\x10key-b\x00"[..], Some(&b"v"[..])),
        ]
    );
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_header_bytes_are_refused() -> TestResult {
    for at in [9usize, 15, 24, 127] {
        let bytes = with_command_header(&golden()?, |h| h[at] = 1);
        assert_eq!(
            refusal::<EntriesView>(&bytes)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_an_end_past_two_is_refused() -> TestResult {
    for (end, expected) in [(0u8, EntriesEnd::More), (2, EntriesEnd::Limit)] {
        let bytes = with_command_header(&golden()?, |h| h[8] = end);
        assert_eq!(EntriesView::from_frame(&frame_of(&bytes)?)?.end(), expected);
    }
    let bytes = with_command_header(&golden()?, |h| h[8] = 3);
    assert_eq!(refusal::<EntriesView>(&bytes)?, field("end", 3));
    Ok(())
}

#[test]
fn test_an_epoch_past_42_bits_is_refused_and_zero_taken() -> TestResult {
    let past = with_command_header(&golden()?, |h| put(h, 16, MAX_EPOCH + 1, 8));
    assert_eq!(
        refusal::<EntriesView>(&past)?,
        field("epoch", MAX_EPOCH + 1)
    );
    for epoch in [0, MAX_EPOCH] {
        let bytes = with_command_header(&golden()?, |h| put(h, 16, epoch, 8));
        assert_eq!(EntriesView::from_frame(&frame_of(&bytes)?)?.epoch(), epoch);
    }
    Ok(())
}

#[test]
fn test_the_end_is_checked_before_the_epoch() -> TestResult {
    let bytes = with_command_header(&golden()?, |h| {
        h[8] = 9;
        put(h, 16, u64::MAX, 8);
    });
    assert_eq!(refusal::<EntriesView>(&bytes)?, field("end", 9));
    Ok(())
}

#[test]
fn test_an_entry_past_its_section_is_refused() -> TestResult {
    let mut short = entry(b"abc", Some(b"def"));
    short[4] = 9;
    let bytes = with_entries(&[entry(b"a", Some(b"1")), short], 2)?;
    assert_eq!(
        refusal::<EntriesView>(&bytes)?,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 525,
            section: 0,
            entry: 1,
            offset: 16
        }
    );
    Ok(())
}

#[test]
fn test_padding_after_an_entry_is_named_at_its_body_offset() -> TestResult {
    let mut padded = entry(b"ab", Some(b"c"));
    padded[12] = 1;
    let bytes = with_entries(&[padded], 1)?;
    // The section starts at body offset 8, the entry's 11 bytes end at 11 and padding byte 12 is set.
    assert_eq!(
        refusal::<EntriesView>(&bytes)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: 525,
            offset: 20
        }
    );
    Ok(())
}

#[test]
fn test_a_removal_is_refused_naming_its_entry() -> TestResult {
    let bytes = with_entries(&[entry(b"a", Some(b"1")), entry(b"b", None)], 2)?;
    assert_eq!(refusal::<EntriesView>(&bytes)?, field("entry", 1));
    Ok(())
}

#[test]
fn test_keys_ascend_byte_by_byte() -> TestResult {
    for keys in [
        [&b"a"[..], &b"a"[..]],
        [&b"b"[..], &b"a"[..]],
        [&b"a\x00"[..], &b"a"[..]],
        [&b"\xFF"[..], &b"\x01\xFF"[..]],
    ] {
        let bytes = with_entries(
            &[
                entry(b"0", Some(b"")),
                entry(keys[0], Some(b"")),
                entry(keys[1], Some(b"")),
            ],
            3,
        )?;
        assert_eq!(refusal::<EntriesView>(&bytes)?, field("entry order", 2));
    }
    let bytes = with_entries(
        &[
            entry(b"", Some(b"")),
            entry(b"a", Some(b"")),
            entry(b"a\x00", Some(b"")),
        ],
        3,
    )?;
    assert_eq!(
        EntriesView::from_frame(&frame_of(&bytes)?)?.entries().len(),
        3
    );
    Ok(())
}

#[test]
fn test_the_count_names_the_entries() -> TestResult {
    for count in [2u32, 4, 0] {
        let bytes = with_command_header(&golden()?, |h| put(h, 0, count.into(), 4));
        assert_eq!(
            refusal::<EntriesView>(&bytes)?,
            ProtocolErrorEnum::CountMismatch {
                command: 525,
                field: "entries",
                count: 3,
                expected: count.into()
            }
        );
    }
    let empty = with_entries(&[], 0)?;
    assert!(
        EntriesView::from_frame(&frame_of(&empty)?)?
            .entries()
            .is_empty()
    );
    assert_eq!(view_any(&empty)?, Ok(()));
    Ok(())
}
