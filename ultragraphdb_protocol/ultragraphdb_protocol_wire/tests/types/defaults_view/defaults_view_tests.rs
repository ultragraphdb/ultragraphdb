/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Defaults' view (`PROTOCOL.md` §13.28), its checks in the Order line's order: unused bytes 16–127 zero; sections;
//! the term table; in a request, no entry; in a reply, each entry of kind 1 and above the one before by its text's
//! bytes. Every refused frame is laid out by hand.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Command, Defaults, DefaultsView};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::defaults::defaults_tests::{GOLDEN_PREDICATES, owned};
use crate::utils::golden_bodies::{frame_of, view_any, with_command_header};
use crate::utils::system_frames::{count, defaults_frame, field, iris, reserved, role};
use crate::utils::term_records::{Entry, refusal};
use crate::utils::ugs_frames::frame;

fn refused(position: u64, entries: &[Entry<'_>]) -> TestResult<ProtocolErrorEnum> {
    refusal::<DefaultsView>(&defaults_frame(3, position, entries, true)?)
}

/// C1.76: the golden frames read back to their fields and build again to the same bytes.
#[test]
fn test_the_golden_defaults_read_and_build_again() -> TestResult {
    let request = role("defaults")?;
    let view = DefaultsView::from_frame(&frame_of(&request)?)?;
    assert_eq!((view.tenant(), view.position()), (3, 0));
    assert!(view.is_request());
    assert_eq!((view.len(), view.is_empty()), (0, true));
    assert_eq!(view.predicate(0), None);
    assert_eq!(view.to_body()?, Defaults::request(3));

    let reply = role("defaults_reply")?;
    let answered = DefaultsView::from_frame(&frame_of(&reply)?)?;
    assert_eq!((answered.tenant(), answered.position()), (3, 1_250));
    assert!(!answered.is_request());
    assert_eq!((answered.len(), answered.is_empty()), (3, false));
    for (index, predicate) in GOLDEN_PREDICATES.into_iter().enumerate() {
        assert_eq!(answered.predicate(index), Some(predicate));
    }
    assert_eq!(answered.predicate(3), None);
    assert_eq!(
        answered.to_body()?,
        Defaults::reply(3, 1_250, owned(&GOLDEN_PREDICATES))
    );
    for bytes in [request, reply] {
        assert_eq!(view_any(&bytes)?, Ok(()));
    }
    Ok(())
}

/// C1.60: an unused byte is refused naming its frame offset.
#[test]
fn test_unused_bytes_are_refused() -> TestResult {
    for at in [16usize, 127] {
        let bytes = with_command_header(&role("defaults_reply")?, |h| h[at] = 1);
        assert_eq!(refusal::<DefaultsView>(&bytes)?, reserved(at));
    }
    Ok(())
}

/// C1.61: the term table is checked by section 4.4 before any rule of the body: its records a whole number of 24
/// bytes, each entry's kind 1 to 5.
#[test]
fn test_the_term_table_is_checked_before_the_body() -> TestResult {
    let records = frame(
        Command::Defaults,
        &[(0, &3u64.to_le_bytes())],
        &[vec![0; 25], Vec::new()],
        true,
    )?;
    assert_eq!(
        refusal::<DefaultsView>(&records)?,
        ProtocolErrorEnum::SectionLength {
            command: 539,
            section: 0,
            len: 25,
            unit: 24
        }
    );
    // A request holding an entry of kind 6: the table's rule, before the request's.
    assert_eq!(
        refused(0, &[Entry::Text(6, "urn:p", "")])?,
        field(539, "term kind", 6)
    );
    Ok(())
}

/// C1.62: a request, `position` 0, holds no entry.
#[test]
fn test_a_request_holding_an_entry_is_refused() -> TestResult {
    assert_eq!(refused(0, &iris(&["urn:p"]))?, count(539, "defaults", 1, 0));
    assert_eq!(
        refused(0, &iris(&["urn:p", "urn:q"]))?,
        count(539, "defaults", 2, 0)
    );
    Ok(())
}

/// C1.63: a reply's entry is an IRI, refused naming its kind.
#[test]
fn test_a_reply_entry_that_is_no_iri_is_refused() -> TestResult {
    let others = [
        (Entry::Text(2, "b1", ""), 2u64),
        (Entry::Text(3, "abc", ""), 3),
        (Entry::Text(4, "chat", "fr"), 4),
    ];
    for (entry, kind) in others {
        assert_eq!(refused(1, &[entry])?, field(539, "default", kind));
    }
    let triple = [
        Entry::Text(1, "urn:a", ""),
        Entry::Text(1, "urn:b", ""),
        Entry::Text(1, "urn:c", ""),
        Entry::Triple(0, 1, 2),
    ];
    assert_eq!(refused(1, &triple)?, field(539, "default", 5));
    // The entries are taken in order: an entry of another kind before one out of order.
    let kind_first = [
        Entry::Text(1, "urn:b", ""),
        Entry::Text(2, "b1", ""),
        Entry::Text(1, "urn:a", ""),
    ];
    assert_eq!(refused(1, &kind_first)?, field(539, "default", 2));
    Ok(())
}

/// C1.64: a reply's entry below the one before by its text's bytes is refused naming its index.
#[test]
fn test_an_entry_below_the_one_before_is_refused() -> TestResult {
    assert_eq!(
        refused(1, &iris(&["urn:b", "urn:a"]))?,
        field(539, "default order", 1)
    );
    assert_eq!(
        refused(1, &iris(&["urn:a", "urn:c", "urn:b"]))?,
        field(539, "default order", 2)
    );
    Ok(())
}

/// C1.65: one IRI twice in one frame is the term table's refusal, checked first (§4.4, step 9).
#[test]
fn test_an_iri_twice_in_one_frame_is_a_duplicate_term() -> TestResult {
    assert_eq!(
        refused(1, &iris(&["urn:a", "urn:a"]))?,
        ProtocolErrorEnum::DuplicateTerm {
            command: 539,
            section: 0,
            at: 1,
            earlier: 0
        }
    );
    Ok(())
}

/// C1.70, C1.71: a reply of no entry, a tenant holding no record, and of one entry read.
#[test]
fn test_a_reply_of_no_entry_and_of_one_reads() -> TestResult {
    let none = defaults_frame(3, 1, &[], true)?;
    let view = DefaultsView::from_frame(&frame_of(&none)?)?;
    assert_eq!((view.position(), view.len(), view.is_empty()), (1, 0, true));
    assert!(!view.is_request());
    let one = defaults_frame(3, u64::MAX, &iris(&["urn:p"]), true)?;
    let single = DefaultsView::from_frame(&frame_of(&one)?)?;
    assert_eq!((single.position(), single.len()), (u64::MAX, 1));
    assert_eq!(single.predicate(0), Some("urn:p"));
    assert_eq!(single.predicate(1), None);
    Ok(())
}

/// C1.72: the order is that of the texts' UTF-8 bytes: `B` (0x42) before `a` (0x61), a text before its extension, `z`
/// (0x7A) before `é` (0xC3 0xA9). Each pair reads in that order and is refused in the other.
#[test]
fn test_the_order_is_of_the_texts_utf8_bytes() -> TestResult {
    for (first, second) in [("B", "a"), ("a", "ab"), ("z", "é")] {
        let bytes = defaults_frame(3, 1, &iris(&[first, second]), true)?;
        let view = DefaultsView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(
            (view.predicate(0), view.predicate(1)),
            (Some(first), Some(second))
        );
        assert_eq!(
            refused(1, &iris(&[second, first]))?,
            field(539, "default order", 1),
            "{second} before {first}"
        );
    }
    Ok(())
}

/// A frame of another command is no Defaults.
#[test]
fn test_a_frame_of_another_command_is_refused() -> TestResult {
    assert_eq!(
        refusal::<DefaultsView>(&role("declare")?)?,
        ProtocolErrorEnum::WrongCommand {
            expected: 539,
            found: 538
        }
    );
    Ok(())
}

/// Phase 3: the term table's own refusals (`PROTOCOL.md` §4.4) reach through the body, each before any rule of a
/// reply: an IRI holding a second text, a text that is not UTF-8, and text past the last entry's.
#[test]
fn test_the_term_tables_refusals_reach_through_the_body() -> TestResult {
    assert_eq!(
        refused(1, &[Entry::Text(1, "urn:a", "x")])?,
        ProtocolErrorEnum::BadTerm {
            command: 539,
            section: 0,
            at: 0
        }
    );
    let (records, mut text) = crate::utils::term_records::term_sections(&iris(&["urn:b", "urn:a"]));
    text[6] = 0xFF;
    let not_utf8 = frame(
        Command::Defaults,
        &[(8, &1u64.to_le_bytes())],
        &[records.clone(), text.clone()],
        true,
    )?;
    assert_eq!(
        refusal::<DefaultsView>(&not_utf8)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 539,
            section: 1,
            offset: 6
        }
    );
    text[6] = b'r';
    text.push(b'!');
    let trailing = frame(
        Command::Defaults,
        &[(8, &1u64.to_le_bytes())],
        &[records, text],
        true,
    )?;
    let refusal = refusal::<DefaultsView>(&trailing)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::CountMismatch {
                command: 539,
                field: "term text",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}
