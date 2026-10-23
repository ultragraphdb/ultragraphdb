/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde_json::Value as Json;
use ultragraphdb_core_dql::{DqlError, parse_dql, parse_mutation};

use super::golden_parse::{Compared, Outcome, check_parse, deviations_listed, golden};
use super::tree_json;

fn check(
    what: &str,
    input: &str,
    want: &Outcome,
    got: Result<Json, DqlError>,
    failures: &mut Vec<String>,
) {
    if !want.admits(&got) {
        failures.push(format!(
            "{what} {input:?}\n  Go:   {}\n  port: {}",
            want.describe(),
            match got {
                Ok(v) => v.to_string(),
                Err(e) => format!("error {e}"),
            }
        ));
    }
}

/// Every string literal of Dgraph's dql tests, parsed as a query (also with every variable map the tests pass,
/// when it names a parameter), as a mutation and as a DQL request: each outcome is one of those Go's runs gave,
/// unstable cases included, or the case is a listed deviation.
#[test]
fn test_golden_parse() {
    let golden = golden();
    let mut failures = Vec::new();
    let mut deviations = 0;
    for c in golden.cases() {
        match check_parse(c) {
            Ok(Compared::AsGo) => {}
            Ok(Compared::Deviation) => deviations += 1,
            Err(e) => failures.push(e),
        }
        if let Some(want) = &c.mutation {
            check(
                "mutation",
                &c.input,
                want,
                parse_mutation(&c.input).map(|r| tree_json::request(&r)),
                &mut failures,
            );
        }
        if let Some(want) = &c.dql {
            check(
                "dql",
                &c.input,
                want,
                parse_dql(&c.input).map(|r| tree_json::request(&r)),
                &mut failures,
            );
        }
    }
    assert!(
        golden.cases().len() > 2000,
        "only {} cases",
        golden.cases().len()
    );
    let trees = golden.cases().iter().filter(|c| c.parse.has_tree()).count();
    assert!(trees > 500, "only {trees} cases where Go parses a tree");
    assert!(
        failures.is_empty(),
        "{} of {} golden cases differ:\n{}",
        failures.len(),
        golden.cases().len(),
        failures[..failures.len().min(25)].join("\n")
    );
    assert_eq!(
        deviations,
        deviations_listed(),
        "every deviation names a golden case"
    );
}
