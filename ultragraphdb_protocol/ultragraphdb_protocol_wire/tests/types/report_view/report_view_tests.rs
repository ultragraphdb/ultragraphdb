/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Report, ReportKind, ReportView, RequestTermView};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::report::report_tests::{AGE, INTEGER, golden_refused};
use crate::utils::golden_bodies::{
    frame_of, join_sections, put, reframe, view_any, with_command_header,
};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::{Entry, refusal, term_sections};

fn golden(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The golden frame `name` with its term table holding `entries`.
fn with_terms(name: &str, entries: &[Entry<'_>]) -> TestResult<Vec<u8>> {
    let (records, text) = term_sections(entries);
    Ok(reframe(
        &golden(name)?,
        &join_sections(&[], &[records, text]),
    ))
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 530,
        field,
        value,
    }
}

fn terms(count: u64, expected: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::CountMismatch {
        command: 530,
        field: "report terms",
        count,
        expected,
    }
}

#[test]
fn test_the_golden_reports_read_in_place() -> TestResult {
    let bytes = golden("report_refused")?;
    let view = ReportView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (
            view.partition(),
            view.kind(),
            view.namespace(),
            view.version()
        ),
        (6, ReportKind::Refused, 4, 12)
    );
    assert_eq!(view.predicate(), Some(AGE));
    assert_eq!(
        view.at_fault(),
        Some(RequestTermView::Literal {
            lexical: "abc",
            datatype: Some(INTEGER)
        })
    );
    assert_eq!(view.terms().len(), 2);
    assert_eq!(view.to_body()?, golden_refused()?);
    let passed = golden("report_passed")?;
    let passed_view = ReportView::from_frame(&frame_of(&passed)?)?;
    assert_eq!(
        (
            passed_view.kind(),
            passed_view.version(),
            passed_view.predicate(),
            passed_view.at_fault()
        ),
        (ReportKind::Passed, 13, None, None)
    );
    assert_eq!(passed_view.to_body()?, Report::passed(6, 4, 13));
    let failed = golden("report_build_failed")?;
    let failed_view = ReportView::from_frame(&frame_of(&failed)?)?;
    assert_eq!(failed_view.predicate(), Some("http://ex.org/embedding"));
    assert_eq!(failed_view.at_fault(), None);
    for frame in [bytes, passed, failed] {
        assert_eq!(view_any(&frame)?, Ok(()));
    }
    Ok(())
}

#[test]
fn test_one_term_names_predicate_and_fault_alike() -> TestResult {
    let bytes = with_terms("report_refused", &[Entry::Text(1, AGE, "")])?;
    let view = ReportView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.at_fault(), Some(RequestTermView::Iri(AGE)));
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_header_bytes_are_refused() -> TestResult {
    for at in [5usize, 7, 24, 127] {
        let bytes = with_command_header(&golden("report_refused")?, |h| h[at] = 1);
        assert_eq!(
            refusal::<ReportView>(&bytes)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_the_fields_are_checked_in_order() -> TestResult {
    let all = with_command_header(&golden("report_refused")?, |h| {
        put(h, 0, 0, 4);
        h[4] = 0;
        put(h, 16, 0, 8);
    });
    assert_eq!(refusal::<ReportView>(&all)?, field("partition", 0));
    for kind in [0u8, 4, u8::MAX] {
        let bytes = with_command_header(&golden("report_refused")?, |h| {
            h[4] = kind;
            put(h, 16, 0, 8);
        });
        assert_eq!(refusal::<ReportView>(&bytes)?, field("kind", kind.into()));
    }
    let bytes = with_command_header(&golden("report_refused")?, |h| put(h, 16, 0, 8));
    assert_eq!(refusal::<ReportView>(&bytes)?, field("version", 0));
    Ok(())
}

#[test]
fn test_each_kind_takes_its_terms() -> TestResult {
    let age = Entry::Text(1, AGE, "");
    let b = Entry::Text(1, "http://ex.org/b", "");
    let c = Entry::Text(1, "http://ex.org/c", "");
    for (name, entries, expected) in [
        ("report_passed", vec![age], terms(1, 0)),
        ("report_refused", vec![], terms(0, 2)),
        ("report_refused", vec![age, b, c], terms(3, 2)),
        ("report_build_failed", vec![], terms(0, 1)),
        ("report_build_failed", vec![age, b], terms(2, 1)),
    ] {
        let bytes = with_terms(name, &entries)?;
        assert_eq!(
            refusal::<ReportView>(&bytes)?,
            expected,
            "{name} {}",
            entries.len()
        );
    }
    Ok(())
}

#[test]
fn test_the_predicate_is_an_iri_and_the_fault_no_triple() -> TestResult {
    let literal = Entry::Text(3, "abc", INTEGER);
    for (name, entries, kind) in [
        ("report_refused", vec![literal], 3u64),
        ("report_build_failed", vec![Entry::Text(2, "b1", "")], 2),
        (
            "report_refused",
            vec![Entry::Text(1, AGE, ""), Entry::Triple(0, 0, 0)],
            5,
        ),
    ] {
        let bytes = with_terms(name, &entries)?;
        assert_eq!(refusal::<ReportView>(&bytes)?, field("report term", kind));
    }
    for second in [
        Entry::Text(2, "b1", ""),
        Entry::Text(4, "chat", "fr"),
        Entry::Text(1, "http://ex.org/s", ""),
    ] {
        let bytes = with_terms("report_refused", &[Entry::Text(1, AGE, ""), second])?;
        assert!(
            ReportView::from_frame(&frame_of(&bytes)?)?
                .at_fault()
                .is_some()
        );
        assert_eq!(view_any(&bytes)?, Ok(()));
    }
    Ok(())
}
