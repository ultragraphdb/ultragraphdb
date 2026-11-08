/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Language, QUERY_TEXT_RESERVE, QueryView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::query::query_tests::{GQL, golden_query};
use crate::utils::golden_bodies::{
    frame_of, golden, put, view_any, with_command_header, with_section,
};
use crate::utils::term_records::refusal;

fn bytes() -> TestResult<Vec<u8>> {
    golden("query")?.bytes()
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 22,
        field,
        value,
    }
}

#[test]
fn test_the_golden_query_reads_its_values() -> TestResult {
    let frame = bytes()?;
    let view = QueryView::from_frame(&frame_of(&frame)?)?;
    assert_eq!(
        (view.transaction(), view.language(), view.limit()),
        (5, Language::Gql, 100)
    );
    assert_eq!(view.text(), GQL);
    assert_eq!(view.parameter_count(), 1);
    assert_eq!(view.parameter(0), Some(("who", 0)));
    assert_eq!(view.parameter(1), None);
    assert_eq!(view.terms().len(), 1);
    assert_eq!(view.resume(), [1, 2]);
    assert_eq!(view.to_body()?, golden_query());
    assert_eq!(view_any(&frame)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_bytes_come_before_the_language() -> TestResult {
    for at in [9usize, 11, 16, 127] {
        let frame = with_command_header(&bytes()?, |h| {
            h[at] = 1;
            h[8] = 9;
        });
        assert_eq!(
            refusal::<QueryView>(&frame)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

/// A Query is a text: language 0, a quad pattern, is a Prepare's alone.
#[test]
fn test_the_language_is_gql_or_sparql() -> TestResult {
    for language in [0u8, 3] {
        let frame = with_command_header(&bytes()?, |h| h[8] = language);
        assert_eq!(
            refusal::<QueryView>(&frame)?,
            field("language", language.into())
        );
    }
    let sparql = with_command_header(&bytes()?, |h| h[8] = 2);
    assert_eq!(
        QueryView::from_frame(&frame_of(&sparql)?)?.language(),
        Language::Sparql
    );
    Ok(())
}

#[test]
fn test_the_values_are_whole_one_per_name_and_in_the_table() -> TestResult {
    let whole = with_section(&bytes()?, 7, 3, |values| values.truncate(3));
    assert!(matches!(
        refusal::<QueryView>(&whole)?,
        ProtocolErrorEnum::SectionLength {
            command: 22,
            section: 3,
            ..
        }
    ));
    let none = with_section(&bytes()?, 7, 3, Vec::clear);
    assert_eq!(
        refusal::<QueryView>(&none)?,
        ProtocolErrorEnum::CountMismatch {
            command: 22,
            field: "parameters",
            count: 0,
            expected: 1
        }
    );
    let past = with_section(&bytes()?, 7, 3, |values| put(values, 0, 1, 4));
    assert_eq!(
        refusal::<QueryView>(&past)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 22,
            section: 3,
            at: 0,
            index: 1,
            limit: 1
        }
    );
    Ok(())
}

#[test]
fn test_the_text_is_utf8() -> TestResult {
    let frame = with_section(&bytes()?, 7, 0, |text| text[0] = 0xFF);
    assert_eq!(
        refusal::<QueryView>(&frame)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 22,
            section: 0,
            offset: 0
        }
    );
    Ok(())
}

#[test]
fn test_the_text_fits_the_receivers_frame() -> TestResult {
    let frame = bytes()?;
    let view = QueryView::from_frame(&frame_of(&frame)?)?;
    let len = u32::try_from(GQL.len())?;
    view.check_text(len + QUERY_TEXT_RESERVE)?;
    assert_eq!(
        view.check_text(len + QUERY_TEXT_RESERVE - 1)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 22,
            field: "query",
            count: len.into(),
            expected: u64::from(len) - 1
        }
    );
    Ok(())
}

/// A query names as many parameters as its names section holds, none to several, and reads each back.
#[test]
fn test_the_parameter_count_is_the_number_of_names() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyBuilder, Query, TermTable};

    for count in [0usize, 3] {
        let mut terms = TermTable::new();
        let values: Vec<u32> = (0..count)
            .map(|at| terms.iri(&format!("http://ex.org/{at}")))
            .collect();
        let query = values.iter().enumerate().fold(
            Query::new(5, Language::Gql, "MATCH (n) RETURN n", terms),
            |query, (at, value)| query.with_parameter(format!("p{at}"), *value),
        );
        let frame = query.frame(golden("query")?.header())?;
        let view = QueryView::from_frame(&frame_of(&frame)?)?;
        assert_eq!(view.parameter_count(), count);
        assert_eq!(view.parameter(count), None);
        assert_eq!(view.to_body()?, query);
    }
    Ok(())
}
