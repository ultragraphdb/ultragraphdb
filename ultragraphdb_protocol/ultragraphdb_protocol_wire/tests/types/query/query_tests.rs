/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Language, Query, TermTable};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

/// The GQL text of the golden Query.
pub const GQL: &str = "MATCH (n) WHERE n = $who RETURN n, n.name";

/// The golden Query: in the transaction of stream 5, limit 100, `who` bound to `<a>`, a resume key.
pub fn golden_query() -> Query {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Query::new(5, Language::Gql, GQL, terms)
        .with_limit(100)
        .with_parameter("who", a)
        .with_resume(vec![1, 2])
}

#[test]
fn test_a_query_builds_the_golden_frame() -> TestResult {
    let query = golden_query();
    assert_eq!(
        (query.transaction(), query.language(), query.limit()),
        (5, Language::Gql, 100)
    );
    assert_eq!(query.text(), GQL);
    assert_eq!(query.parameters(), [("who".to_string(), 0)]);
    assert_eq!(query.terms().len(), 1);
    assert_eq!(query.resume(), [1, 2]);
    assert_eq!(
        query.frame(golden("query")?.header())?,
        golden("query")?.bytes()?
    );
    Ok(())
}
