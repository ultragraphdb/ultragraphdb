/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value as Json;
use ultragraphdb_core_dql::{DqlError, Request, parse, parse_dql, parse_mutation};

use super::tree_json;

#[derive(Debug, Deserialize)]
struct Outcome {
    ok: Json,
    err: String,
    is_err: bool,
    stable: bool,
}

#[derive(Debug, Deserialize)]
struct Case {
    input: String,
    variables: Option<HashMap<String, String>>,
    parse: Outcome,
    mutation: Outcome,
    dql: Outcome,
}

fn check(
    what: &str,
    input: &str,
    want: &Outcome,
    got: Result<Json, DqlError>,
    failures: &mut Vec<String>,
) {
    if !want.stable {
        return;
    }
    let ok = match (&got, want.is_err) {
        (Err(e), true) => e.to_string() == want.err,
        (Ok(v), false) => *v == want.ok,
        _ => false,
    };
    if !ok {
        failures.push(format!(
            "{what} {input:?}\n  want: {}\n  got:  {}",
            if want.is_err {
                want.err.clone()
            } else {
                want.ok.to_string()
            },
            match got {
                Ok(v) => v.to_string(),
                Err(e) => format!("error {e}"),
            }
        ));
    }
}

/// Every string literal of Dgraph's dql tests, parsed as a query (also with every variable map the tests pass,
/// when it names a parameter), as a mutation and as a DQL request: the trees and error texts are Dgraph's.
#[test]
fn test_golden_parse() {
    let path =
        ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_dql").join("golden/dql_parse.json");
    let golden = ultragraphdb_test_bdd::load_golden::<Case>(&path);
    let mut failures = Vec::new();
    let mut compared = 0;
    for c in golden.cases() {
        let req = match &c.variables {
            Some(vars) => Request::with_variables(c.input.clone(), vars.clone()),
            None => Request::new(c.input.clone()),
        };
        check(
            "parse",
            &c.input,
            &c.parse,
            parse(&req).map(|r| tree_json::result(&r)),
            &mut failures,
        );
        compared += usize::from(c.parse.stable);
        if c.variables.is_none() {
            check(
                "mutation",
                &c.input,
                &c.mutation,
                parse_mutation(&c.input).map(|r| tree_json::request(&r)),
                &mut failures,
            );
            check(
                "dql",
                &c.input,
                &c.dql,
                parse_dql(&c.input).map(|r| tree_json::request(&r)),
                &mut failures,
            );
        }
    }
    assert!(compared > 1500, "only {compared} stable parse cases");
    assert!(
        failures.is_empty(),
        "{} of {} golden cases differ:\n{}",
        failures.len(),
        golden.cases().len(),
        failures[..failures.len().min(25)].join("\n")
    );
}
