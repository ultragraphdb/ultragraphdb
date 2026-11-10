/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::TaggedDatatype;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §10.4: codes 34 `ugdb:bfloat16vector`, 35 `ugdb:measure`, 36 `ugdb:precision`, `ugdb:` being
/// `https://w3id.org/ultragraphdb/ns/`; 33 and 37 are no code of theirs.
#[test]
fn test_codes_34_to_36_name_the_three_datatypes() -> TestResult {
    let table = [
        (34, TaggedDatatype::Bfloat16Vector, "https://w3id.org/ultragraphdb/ns/bfloat16vector"),
        (35, TaggedDatatype::Measure, "https://w3id.org/ultragraphdb/ns/measure"),
        (36, TaggedDatatype::Precision, "https://w3id.org/ultragraphdb/ns/precision"),
    ];
    for (code, datatype, iri) in table {
        assert_eq!(TaggedDatatype::from_code(code), Some(datatype));
        assert_eq!((datatype.code(), datatype.iri()), (code, iri));
    }
    assert_eq!(TaggedDatatype::from_code(33), None);
    assert_eq!(TaggedDatatype::from_code(37), None);
    Ok(())
}
