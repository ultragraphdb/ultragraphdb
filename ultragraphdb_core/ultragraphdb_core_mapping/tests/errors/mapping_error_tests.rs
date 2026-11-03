/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{MappingError, MappingErrorEnum};
use ultragraphdb_tool_bdd::TestResult;

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
            "t is held by several statements; name one by its reifier",
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

#[test]
fn test_a_statement_id_of_another_type_is_named_by_its_id() -> TestResult {
    // Type 0001, sequence 0000000000000009, bucket 00002a, reserved 000000.
    let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(42)?, Sequence::new(9)?);
    let error = MappingError::not_a_statement_id(id);
    assert_eq!(error.0, MappingErrorEnum::NotAStatementId { id });
    assert_eq!(
        error.to_string(),
        "The ID 0001000000000000000900002a000000 is not of the Statement type"
    );
    Ok(())
}
