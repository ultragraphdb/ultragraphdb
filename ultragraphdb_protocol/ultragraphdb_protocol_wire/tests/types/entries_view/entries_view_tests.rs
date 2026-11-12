/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Entries, EntriesEnd, EntriesView, MAX_EPOCH,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::entries::entries_tests::{
    EPOCH, LIST_A, LIST_B, VERSION, golden_entries_continued, golden_entries_continues,
};
use crate::utils::golden_bodies::{
    body_data, frame_of, put, reframe, view_any, with_command_header, with_section,
};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::refusal;

fn golden() -> TestResult<Vec<u8>> {
    role_bytes(&golden_role("entries")?)
}

/// An entry laid out from the key-value table: `key_len` u32, `value_len` u32 (`u32::MAX` for none), the key, the
/// value, zeros to a multiple of 8.
fn entry(key: &[u8], value: Option<&[u8]>) -> TestResult<Vec<u8>> {
    let mut out = u32::try_from(key.len())?.to_le_bytes().to_vec();
    let value_len = value.map_or(Ok(u32::MAX), |v| u32::try_from(v.len()))?;
    out.extend_from_slice(&value_len.to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(value.unwrap_or_default());
    out.resize(out.len().div_ceil(8) * 8, 0);
    Ok(out)
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

/// `bytes` with one byte more at the end of its body.
fn with_trailing_byte(bytes: &[u8]) -> Vec<u8> {
    let mut data = body_data(bytes).to_vec();
    data.push(0);
    reframe(bytes, &data)
}

/// The refusal `CountMismatch` `"entries"` of `count` entries against the `entries` field's `expected`.
fn count_refused(count: u64, expected: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::CountMismatch {
        command: 525,
        field: "entries",
        count,
        expected,
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
fn test_the_golden_frames_across_a_list_read_back_to_their_bodies() -> TestResult {
    let first = role_bytes(&golden_role("entries_continues")?)?;
    let view = EntriesView::from_frame(&frame_of(&first)?)?;
    assert!(view.continues());
    assert_eq!(view.end(), EntriesEnd::More);
    assert_eq!((view.range(), view.epoch()), (0, EPOCH));
    let keys: Vec<&[u8]> = view.entries().iter().map(|entry| entry.key()).collect();
    assert_eq!(keys, [LIST_A, LIST_B]);
    assert_eq!(view.to_body()?, golden_entries_continues());
    assert_eq!(view_any(&first)?, Ok(()));
    let second = role_bytes(&golden_role("entries_continued")?)?;
    let next = EntriesView::from_frame(&frame_of(&second)?)?;
    assert!(!next.continues());
    assert_eq!(next.end(), EntriesEnd::Ended);
    assert_eq!(next.to_body()?, golden_entries_continued());
    assert_eq!(view_any(&second)?, Ok(()));
    Ok(())
}

#[test]
fn test_continues_is_a_boolean() -> TestResult {
    let first = role_bytes(&golden_role("entries_continues")?)?;
    for value in [2u8, 255] {
        let bytes = with_command_header(&first, |h| h[9] = value);
        assert_eq!(
            refusal::<EntriesView>(&bytes)?,
            field("continues", value.into())
        );
    }
    Ok(())
}

#[test]
fn test_continues_is_0_when_end_is_not_0() -> TestResult {
    let first = role_bytes(&golden_role("entries_continues")?)?;
    for end in [EntriesEnd::Ended, EntriesEnd::Limit] {
        let bytes = with_command_header(&first, |h| h[8] = end.number());
        assert_eq!(refusal::<EntriesView>(&bytes)?, field("continues", 1));
    }
    Ok(())
}

#[test]
fn test_the_order_is_unused_bytes_end_continues_epoch() -> TestResult {
    let bytes = with_command_header(&golden()?, |h| {
        h[10] = 1;
        h[8] = 3;
        h[9] = 2;
        put(h, 16, MAX_EPOCH + 1, 8);
    });
    assert_eq!(
        refusal::<EntriesView>(&bytes)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 10 }
    );
    let end = with_command_header(&bytes, |h| h[10] = 0);
    assert_eq!(refusal::<EntriesView>(&end)?, field("end", 3));
    let continues = with_command_header(&end, |h| h[8] = 0);
    assert_eq!(refusal::<EntriesView>(&continues)?, field("continues", 2));
    let epoch = with_command_header(&continues, |h| h[9] = 1);
    assert_eq!(
        refusal::<EntriesView>(&epoch)?,
        field("epoch", MAX_EPOCH + 1)
    );
    Ok(())
}

/// `continues` 0 when `end` is not 0 is part of the `continues` clause, before `epoch`.
#[test]
fn test_continues_with_an_end_comes_before_the_epoch() -> TestResult {
    let first = role_bytes(&golden_role("entries_continues")?)?;
    let bytes = with_command_header(&first, |h| {
        h[8] = EntriesEnd::Ended.number();
        put(h, 16, MAX_EPOCH + 1, 8);
    });
    assert_eq!(refusal::<EntriesView>(&bytes)?, field("continues", 1));
    Ok(())
}

/// The sections come after the header's rules and before the entries.
#[test]
fn test_the_sections_lie_between_the_header_and_the_entries() -> TestResult {
    let trailing = with_trailing_byte(&golden()?);
    let whole = body_data(&golden()?).len();
    assert_eq!(
        refusal::<EntriesView>(&trailing)?,
        ProtocolErrorEnum::TrailingBytes {
            command: 525,
            end: whole,
            len: whole + 1
        }
    );
    let bad_epoch = with_command_header(&trailing, |h| put(h, 16, MAX_EPOCH + 1, 8));
    assert_eq!(
        refusal::<EntriesView>(&bad_epoch)?,
        field("epoch", MAX_EPOCH + 1)
    );
    let removal = with_entries(&[entry(b"a", None)?], 1)?;
    let holding_it = body_data(&removal).len();
    assert_eq!(
        refusal::<EntriesView>(&with_trailing_byte(&removal))?,
        ProtocolErrorEnum::TrailingBytes {
            command: 525,
            end: holding_it,
            len: holding_it + 1
        }
    );
    Ok(())
}

#[test]
fn test_unused_header_bytes_are_refused() -> TestResult {
    for at in [10usize, 11, 32, 127] {
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
    let mut short = entry(b"abc", Some(b"def"))?;
    short[4] = 9;
    let bytes = with_entries(&[entry(b"a", Some(b"1"))?, short], 2)?;
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
    let mut padded = entry(b"ab", Some(b"c"))?;
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
    let bytes = with_entries(&[entry(b"a", Some(b"1"))?, entry(b"b", None)?], 2)?;
    assert_eq!(refusal::<EntriesView>(&bytes)?, field("entry", 1));
    Ok(())
}

#[test]
fn test_every_value_present_comes_before_the_keys_order() -> TestResult {
    let bytes = with_entries(
        &[
            entry(b"b", Some(b"1"))?,
            entry(b"a", Some(b"2"))?,
            entry(b"c", None)?,
        ],
        3,
    )?;
    assert_eq!(refusal::<EntriesView>(&bytes)?, field("entry", 2));
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
                entry(b"0", Some(b""))?,
                entry(keys[0], Some(b""))?,
                entry(keys[1], Some(b""))?,
            ],
            3,
        )?;
        assert_eq!(refusal::<EntriesView>(&bytes)?, field("entry order", 2));
    }
    let bytes = with_entries(
        &[
            entry(b"", Some(b""))?,
            entry(b"a", Some(b""))?,
            entry(b"a\x00", Some(b""))?,
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
            count_refused(3, count.into())
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

#[test]
fn test_continues_needs_an_entry() -> TestResult {
    let continuing = with_command_header(&with_entries(&[], 0)?, |h| {
        h[8] = EntriesEnd::More.number();
        h[9] = 1;
    });
    assert_eq!(refusal::<EntriesView>(&continuing)?, field("continues", 1));
    Ok(())
}

/// A builder given nothing writes what it was given, and the view refuses the frame.
#[test]
fn test_a_continuing_builder_given_no_entry_builds_a_frame_the_view_refuses() -> TestResult {
    let bytes = Entries::continuing(0, EPOCH).frame(golden_role("entries")?.header())?;
    assert_eq!(refusal::<EntriesView>(&bytes)?, field("continues", 1));
    Ok(())
}

/// An entry being wanted is the last check of all, after the count.
#[test]
fn test_continues_needing_an_entry_comes_after_the_count() -> TestResult {
    let continuing = with_command_header(&with_entries(&[], 1)?, |h| {
        h[8] = EntriesEnd::More.number();
        h[9] = 1;
    });
    assert_eq!(refusal::<EntriesView>(&continuing)?, count_refused(0, 1));
    Ok(())
}

/// The count is the last check of a frame's entries: a wrong one comes after every entry's own rule.
#[test]
fn test_the_count_comes_after_the_entries() -> TestResult {
    let removal = [entry(b"a", Some(b"1"))?, entry(b"b", None)?];
    let wrong_count = u32::try_from(removal.len())? + 1;
    assert_eq!(
        refusal::<EntriesView>(&with_entries(&removal, wrong_count)?)?,
        field("entry", 1)
    );
    let unordered = [entry(b"b", Some(b"1"))?, entry(b"a", Some(b"2"))?];
    assert_eq!(
        refusal::<EntriesView>(&with_entries(&unordered, wrong_count)?)?,
        field("entry order", 1)
    );
    let mended = [entry(b"a", Some(b"1"))?, entry(b"b", Some(b"2"))?];
    assert_eq!(
        refusal::<EntriesView>(&with_entries(&mended, wrong_count)?)?,
        count_refused(mended.len() as u64, wrong_count.into())
    );
    Ok(())
}

fn run() -> TestResult<Vec<u8>> {
    role_bytes(&golden_role("entries_run")?)
}

#[test]
fn test_the_golden_run_reads_its_ranges_and_version() -> TestResult {
    let bytes = run()?;
    let view = EntriesView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.range(), view.through()), (2, 5));
    assert_eq!(view.end(), EntriesEnd::Ended);
    assert_eq!(view.version(), VERSION);
    assert_eq!(view.entries().len(), 2);
    let keys: Vec<&[u8]> = view.entries().iter().map(|entry| entry.key()).collect();
    assert!(
        keys[0] > keys[1],
        "a run follows its ranges' order, not the keys'"
    );
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_batch_of_one_range_reads_through_equal_to_range() -> TestResult {
    let bytes = golden()?;
    let view = EntriesView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.range(), view.through()), (2, 2));
    assert_eq!(view.to_body()?.through(), 2);
    Ok(())
}

#[test]
fn test_through_is_not_below_range_and_above_it_ends_the_range() -> TestResult {
    let below = with_command_header(&golden()?, |h| put(h, 12, 1, 4));
    assert_eq!(refusal::<EntriesView>(&below)?, field("through", 1));
    for end in [0u8, 2] {
        let open = with_command_header(&run()?, |h| h[8] = end);
        assert_eq!(refusal::<EntriesView>(&open)?, field("through", 5));
    }
    Ok(())
}

/// A batch of one range holds its keys in order; a run holds them in its ranges'.
#[test]
fn test_only_a_batch_of_one_range_holds_its_keys_ascending() -> TestResult {
    let one = with_command_header(&run()?, |h| put(h, 12, 2, 4));
    assert_eq!(refusal::<EntriesView>(&one)?, field("entry order", 1));
    Ok(())
}

#[test]
fn test_a_version_holds_an_even_sequence_or_a_bound() -> TestResult {
    for odd in [(EPOCH << 22) | 3, (EPOCH << 22) | 0x3F_FFFD] {
        let refused = with_command_header(&golden()?, |h| put(h, 24, odd, 8));
        assert_eq!(refusal::<EntriesView>(&refused)?, field("version", odd));
    }
    for version in [0, 2, EPOCH << 22, VERSION] {
        let read = with_command_header(&golden()?, |h| put(h, 24, version, 8));
        assert_eq!(
            EntriesView::from_frame(&frame_of(&read)?)?.version(),
            version
        );
    }
    Ok(())
}

#[test]
fn test_through_and_the_epoch_come_before_the_version() -> TestResult {
    let frame = with_command_header(&golden()?, |h| {
        put(h, 12, 1, 4);
        put(h, 16, MAX_EPOCH + 1, 8);
        put(h, 24, 1, 8);
    });
    assert_eq!(refusal::<EntriesView>(&frame)?, field("through", 1));
    let epoch_first = with_command_header(&golden()?, |h| {
        put(h, 16, MAX_EPOCH + 1, 8);
        put(h, 24, 1, 8);
    });
    assert_eq!(
        refusal::<EntriesView>(&epoch_first)?,
        field("epoch", MAX_EPOCH + 1)
    );
    Ok(())
}
