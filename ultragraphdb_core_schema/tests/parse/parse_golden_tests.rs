/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Every schema of the golden corpus parses as Dgraph's `Parse` and `ParseWithNamespace(s, 5)` parse it.

use ultragraphdb_core_schema::{parse, parse_with_namespace};

use crate::golden_json::{compare_outcome, parse_cases};

#[test]
fn golden_corpus_parses_as_dgraph() {
    let golden = parse_cases();
    assert!(golden.cases().len() > 2000);
    let mismatches: Vec<String> = golden
        .cases()
        .iter()
        .flat_map(|c| {
            [
                compare_outcome(&parse(&c.input), &c.parse)
                    .err()
                    .map(|m| format!("parse {:?}: {m}", c.input)),
                compare_outcome(&parse_with_namespace(&c.input, 5), &c.ns5)
                    .err()
                    .map(|m| format!("ns5 {:?}: {m}", c.input)),
            ]
        })
        .flatten()
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
