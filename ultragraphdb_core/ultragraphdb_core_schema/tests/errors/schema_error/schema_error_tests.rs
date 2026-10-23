/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_schema::{SchemaErrorEnum, SchemaMode, SchemaState, parse};

#[test]
fn lexing_errors_keep_the_lexer_text() {
    let e = parse("name: string $ .").unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Lexing(_)));
    assert_eq!(
        e.to_string(),
        "while lexing name: string $ . at line 1 column 13: Invalid schema. Unexpected $"
    );
}

#[test]
fn located_errors_carry_the_position() {
    let e = parse("name: foo .").unwrap_err();
    match &e.0 {
        SchemaErrorEnum::Located {
            line,
            column,
            message,
        } => {
            assert_eq!((*line, *column), (1, 6));
            assert_eq!(message, "Undefined Type");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(e.to_string(), "line 1 column 6: Undefined Type");
}

#[test]
fn unlocated_errors_are_parse_errors() {
    let e = parse("[abc] name: string .").unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Parse(_)));
    assert_eq!(
        e.to_string(),
        "While parsing namespace:: Typed oes not match the expected"
    );
}

#[test]
fn state_errors_name_the_predicate() {
    let state = SchemaState::new();
    let e = state.type_of("0-agea").unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::State(_)));
    assert_eq!(e.to_string(), "Schema not defined for predicate: 0-agea.");
    let e = state
        .factory_create_spec(SchemaMode::Query, "0-agea")
        .unwrap_err();
    assert_eq!(e.to_string(), "Schema state not found for 0-agea.");
    let e: &dyn std::error::Error = &e;
    assert!(e.source().is_none());
}
