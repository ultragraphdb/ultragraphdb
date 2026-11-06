/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_frontend::check_table;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, FrameView, Header, MAX_FRAME_SIZE, RequestTerm,
    TermTable, Write, WriteChange, WriteView,
};
use ultragraphdb_tool_bdd::TestResult;

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

const CLUSTER: u64 = 1;

/// What `check_table` says of a literal of `datatype`: its error code, `None` when it holds.
fn checked(lexical: &str, datatype: &str) -> TestResult<Option<ErrorCode>> {
    let mut table = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: table.iri("http://ex.org/a"),
        predicate: table.iri("http://ex.org/p"),
        object: table.intern(RequestTerm::Literal {
            lexical: lexical.to_string(),
            datatype: Some(format!("{XSD}{datatype}")),
        })?,
    };
    let frame = Write::new(table, vec![change]).frame(Header::new(CLUSTER, Command::Write))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    Ok(check_table(write.terms())
        .err()
        .map(|refused| refused.code()))
}

/// A decimal or an integer past what a by-value payload holds is in its lexical space, and kept by its lexical form.
#[test]
fn test_a_number_past_every_payload_is_still_a_lexical_form() -> TestResult {
    for (lexical, datatype) in [
        ("0.1234567890123456789", "decimal"),
        ("123456789012345678901234567890.5", "decimal"),
        ("-.5", "decimal"),
        ("+5.", "decimal"),
        ("99999999999999999999999", "integer"),
        ("-99999999999999999999999", "long"),
    ] {
        assert_eq!(checked(lexical, datatype)?, None, "{lexical}");
    }
    Ok(())
}

/// What lies outside a number's lexical space is refused.
#[test]
fn test_what_is_no_number_is_refused() -> TestResult {
    for (lexical, datatype) in [
        ("", "decimal"),
        (".", "decimal"),
        ("+", "decimal"),
        ("1e5", "decimal"),
        ("1.2.3", "decimal"),
        ("", "integer"),
        ("-", "integer"),
        ("1.5", "integer"),
        ("twelve", "integer"),
    ] {
        assert_eq!(
            checked(lexical, datatype)?,
            Some(ErrorCode::Refused),
            "{lexical}"
        );
    }
    Ok(())
}
