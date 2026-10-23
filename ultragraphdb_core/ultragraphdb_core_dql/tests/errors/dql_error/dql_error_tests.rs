/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_dql::{DqlErrorEnum, Request, parse, parse_uid};

#[test]
fn test_lexer_error_keeps_its_text() {
    let err = parse(&Request::new("{ me(func: uid(1)) { name }")).unwrap_err();
    assert!(matches!(err.0, DqlErrorEnum::Lexing(_)));
    assert!(
        err.to_string()
            .starts_with("while lexing { me(func: uid(1)) { name } at line 1 column")
    );
}

#[test]
fn test_parser_error_is_located() {
    let err = parse(&Request::new("{ me(func: uid(1)) { name@en@ru } }")).unwrap_err();
    assert_eq!(
        err.0,
        DqlErrorEnum::Located {
            line: 1,
            column: 29,
            message: "Expected directive or language list, got @ru".into(),
        }
    );
    assert_eq!(
        err.to_string(),
        "line 1 column 29: Expected directive or language list, got @ru"
    );
}

#[test]
fn test_unlocated_errors() {
    let err = parse(&Request::new("{ me(func: uid(a)) { name } }")).unwrap_err();
    assert!(matches!(err.0, DqlErrorEnum::Parse(_)));
    let err = parse_uid("0").unwrap_err();
    assert_eq!(
        err.0,
        DqlErrorEnum::Conversion("UID must to be greater than 0".into())
    );
    let _: &dyn std::error::Error = &err;
}
