/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_mapping::MappingError;

#[test]
fn test_each_error_says_what_was_refused() {
    let cases = [
        (
            MappingError::bad_contract_version(&[1]),
            "Unreadable mapping contract version [01]",
        ),
        (
            MappingError::no_such_statement("5".into()),
            "No statement 5 is held",
        ),
        (
            MappingError::ambiguous_statement("t".into()),
            "t is held by several statements; name one by its statement ID",
        ),
        (
            MappingError::unsupported_object("<< >>".into()),
            "The object << >> is a triple term, which no write stores",
        ),
        (
            MappingError::invalid_base("ex.org/".into(), "not an absolute IRI"),
            "Invalid base IRI \"ex.org/\": not an absolute IRI",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}
