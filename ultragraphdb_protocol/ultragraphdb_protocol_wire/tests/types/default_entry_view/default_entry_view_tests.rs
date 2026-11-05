/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, DefaultEntry, DefaultEntryView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{
    frame_of, join_sections, reframe, view_any, with_command_header,
};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::{Entry, refusal, term_sections};

fn golden(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The golden request with its term table holding `entries`.
fn with_terms(entries: &[Entry<'_>]) -> TestResult<Vec<u8>> {
    let (records, text) = term_sections(entries);
    Ok(reframe(
        &golden("default_entry")?,
        &join_sections(&[], &[records, text]),
    ))
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 529,
        field,
        value,
    }
}

#[test]
fn test_the_golden_request_and_reply_read() -> TestResult {
    let request = golden("default_entry")?;
    let view = DefaultEntryView::from_frame(&frame_of(&request)?)?;
    assert_eq!(view.tenant(), 3);
    assert!(!view.found());
    assert_eq!(view.predicate(), "http://ex.org/age");
    assert_eq!(view.check_request(), Ok(()));
    assert_eq!(
        view.to_body()?,
        DefaultEntry::request(3, "http://ex.org/age")
    );
    let reply = golden("default_entry_reply")?;
    let reply_view = DefaultEntryView::from_frame(&frame_of(&reply)?)?;
    assert!(reply_view.found());
    assert_eq!(
        reply_view.check_request().map_err(|e| e.0),
        Err(field("found", 1))
    );
    for bytes in [request, reply] {
        assert_eq!(view_any(&bytes)?, Ok(()));
    }
    Ok(())
}

#[test]
fn test_unused_header_bytes_and_found_are_checked() -> TestResult {
    for at in [9usize, 127] {
        let bytes = with_command_header(&golden("default_entry")?, |h| h[at] = 1);
        assert_eq!(
            refusal::<DefaultEntryView>(&bytes)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let bytes = with_command_header(&golden("default_entry")?, |h| h[8] = 2);
    assert_eq!(refusal::<DefaultEntryView>(&bytes)?, field("found", 2));
    Ok(())
}

#[test]
fn test_the_table_holds_one_entry() -> TestResult {
    for (entries, count) in [
        (vec![], 0u64),
        (
            vec![
                Entry::Text(1, "http://ex.org/a", ""),
                Entry::Text(1, "http://ex.org/b", ""),
            ],
            2,
        ),
    ] {
        let bytes = with_terms(&entries)?;
        assert_eq!(
            refusal::<DefaultEntryView>(&bytes)?,
            ProtocolErrorEnum::CountMismatch {
                command: 529,
                field: "default entry",
                count,
                expected: 1
            }
        );
    }
    Ok(())
}

#[test]
fn test_the_entry_is_an_iri() -> TestResult {
    for (entry, kind) in [
        (Entry::Text(2, "b1", ""), 2u64),
        (
            Entry::Text(3, "35", "http://www.w3.org/2001/XMLSchema#integer"),
            3,
        ),
        (Entry::Text(4, "chat", "fr"), 4),
    ] {
        let bytes = with_terms(&[entry])?;
        assert_eq!(
            refusal::<DefaultEntryView>(&bytes)?,
            field("default entry", kind)
        );
    }
    Ok(())
}

#[test]
fn test_a_malformed_table_is_refused_by_its_rules() -> TestResult {
    let bytes = with_terms(&[Entry::Text(9, "x", "")])?;
    assert_eq!(refusal::<DefaultEntryView>(&bytes)?, field("term kind", 9));
    Ok(())
}
