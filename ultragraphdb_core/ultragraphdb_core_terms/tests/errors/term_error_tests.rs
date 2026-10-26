/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{TermError, TermErrorEnum};

#[test]
fn test_each_error_names_what_it_refused() {
    let cases = [
        (
            TermError::Truncated(vec![0x20, 3]),
            "Encoded term 2003 ends before its tag says it does",
        ),
        (TermError::UnknownTag(0x7f), "Unknown term tag 0x7f"),
        (
            TermError::UnknownDatatype(200),
            "Unknown datatype code 0xc8",
        ),
        (
            TermError::Malformed(vec![0xff], "not UTF-8"),
            "Malformed encoded term ff: not UTF-8",
        ),
        (
            TermError::InvalidVector("[]".into(), "empty"),
            "Invalid vector literal \"[]\": empty",
        ),
        (
            TermError::NotALiteral(7),
            "Encoded term names UID 7, not a literal",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
    assert!(matches!(
        TermError::UnknownTag(1).0,
        TermErrorEnum::UnknownTag { tag: 1 }
    ));
    let boxed: Box<dyn std::error::Error> = Box::new(TermError::UnknownTag(1));
    assert!(boxed.source().is_none());
}
