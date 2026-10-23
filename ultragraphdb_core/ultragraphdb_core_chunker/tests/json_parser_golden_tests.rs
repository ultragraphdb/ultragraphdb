/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/chunker.go`: NQuadBuffer.ParseJSON.

use std::collections::BTreeSet;

use ultragraphdb_core_chunker::{JsonOp, parse_json};
use ultragraphdb_proto_dgraph::api;

use crate::golden_nquad::{JsonCase, canonical, golden, nquad_json, unhex};

/// What the port gives where it deliberately differs from Go.
enum Expected {
    /// Parsing fails with this text.
    Error(&'static str),
    /// Parsing succeeds, and every N-Quad has this predicate and language.
    PredicateLang(&'static str, &'static str),
}

/// The cases whose port result differs from Go's: the case, the entry of
/// `openspec/notes/dgraph-bugs-found-in-port.md`, and the port's result, for set and delete alike.
///
/// Entry 22: a JSON mutation is one JSON value, and anything but whitespace after it is refused, where Go keeps the
/// first object, or reads the input again after a root that is not an object and keeps or refuses what it finds.
const DEVIATIONS: &[(&str, u32, Expected)] = &[
    (
        "trailing data",
        22,
        Expected::Error("invalid character 'x' after top-level value"),
    ),
    (
        "trailing value after list",
        22,
        Expected::Error("invalid character '[' after top-level value"),
    ),
    (
        "trailing garbage after list",
        22,
        Expected::Error("invalid character 'x' after top-level value"),
    ),
    (
        "scan literal true true",
        22,
        Expected::Error("invalid character 't' after top-level value"),
    ),
    (
        "scan number leading zero",
        22,
        Expected::Error("invalid character '1' after top-level value"),
    ),
    (
        "trailing x",
        22,
        Expected::Error("invalid character 'x' after top-level value"),
    ),
    (
        "trailing comma",
        22,
        Expected::Error("invalid character ',' after top-level value"),
    ),
    (
        "trailing close bracket",
        22,
        Expected::Error("invalid character ']' after top-level value"),
    ),
    (
        "trailing close brace",
        22,
        Expected::Error("invalid character '}' after top-level value"),
    ),
    (
        "trailing object",
        22,
        Expected::Error("invalid character '{' after top-level value"),
    ),
    (
        "trailing open brace",
        22,
        Expected::Error("invalid character '{' after top-level value"),
    ),
    (
        "trailing nul",
        22,
        Expected::Error("invalid character '\\x00' after top-level value"),
    ),
    (
        "trailing invalid byte",
        22,
        Expected::Error("invalid character '\\xff' after top-level value"),
    ),
    (
        "trailing non-ascii",
        22,
        Expected::Error("invalid character 'é' after top-level value"),
    ),
    (
        "trailing number",
        22,
        Expected::Error("invalid character '1' after top-level value"),
    ),
    (
        "trailing after null",
        22,
        Expected::Error("invalid character 'x' after top-level value"),
    ),
    (
        "trailing after empty list",
        22,
        Expected::Error("invalid character 'x' after top-level value"),
    ),
    (
        "trailing after string",
        22,
        Expected::Error("invalid character 'x' after top-level value"),
    ),
    (
        "trailing after number",
        22,
        Expected::Error("invalid character '2' after top-level value"),
    ),
    (
        "trailing after list syntax error",
        22,
        Expected::Error("invalid character '[' after top-level value"),
    ),
    // Entry 23: the items of a list under `predicate@lang` carry the language, as every other value does.
    (
        "list with language",
        23,
        Expected::PredicateLang("tags", "en"),
    ),
];

fn op_of(c: &JsonCase) -> JsonOp {
    if c.op == 0 {
        JsonOp::Set
    } else {
        JsonOp::Delete
    }
}

/// The port's result against a deviation's expectation.
fn check_deviation(c: &JsonCase, want: &Expected) -> Result<(), String> {
    let op = op_of(c);
    match (parse_json(&unhex(&c.input_hex), op), want) {
        (Err(e), Expected::Error(text)) if e.to_string() == *text => Ok(()),
        (Ok((nqs, _)), Expected::PredicateLang(p, l))
            if !nqs.is_empty()
                && nqs
                    .iter()
                    .all(|nq: &api::NQuad| nq.predicate == *p && nq.lang == *l) =>
        {
            Ok(())
        }
        (got, _) => Err(format!("{} ({op:?}): deviation: {got:?}", c.name)),
    }
}

fn check_json(c: &JsonCase) -> Result<(), String> {
    if let Some((_, _, want)) = DEVIATIONS.iter().find(|(name, _, _)| *name == c.name) {
        return check_deviation(c, want);
    }
    let op = op_of(c);
    match parse_json(&unhex(&c.input_hex), op) {
        Ok((nqs, md)) => {
            if !c.err.is_empty() {
                return Err(format!("{} ({op:?}): {nqs:?}, want {:?}", c.name, c.err));
            }
            let got: Vec<_> = nqs.iter().map(|nq| nquad_json(nq, true)).collect();
            let blanks: BTreeSet<&str> = got
                .iter()
                .flat_map(|nq| [nq.subject.as_str(), nq.object_id.as_str()])
                .filter(|s| s.starts_with("_:dg."))
                .collect();
            if canonical(got.clone()) != canonical(c.nquads.clone()) || blanks.len() != c.blanks {
                return Err(format!("{} ({op:?}): {got:?}, want {:?}", c.name, c.nquads));
            }
            let mut hints: Vec<(String, i32)> = md.pred_hints.into_iter().collect();
            hints.sort();
            let want: Vec<(String, i32)> =
                c.hints.iter().map(|h| (h.pred.clone(), h.hint)).collect();
            if hints != want {
                return Err(format!(
                    "{} ({op:?}): hints {hints:?}, want {want:?}",
                    c.name
                ));
            }
            Ok(())
        }
        // Dgraph's text, one of those Go gave over its runs: which error comes first depends on Go's map order.
        Err(e) if c.errs.contains(&e.to_string()) => Ok(()),
        Err(e) => Err(format!("{} ({op:?}): {e}, want {:?}", c.name, c.errs)),
    }
}

#[test]
fn parse_json_matches_go() {
    let g = golden::<JsonCase>("chunker_json");
    assert!(g.cases().len() > 500);
    assert!(
        g.cases()
            .iter()
            .filter(|c| c.err.is_empty())
            .all(|c| c.stable)
    );
    let failures: Vec<String> = g
        .cases()
        .iter()
        .filter_map(|c| check_json(c).err())
        .collect();
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_deviation_names_a_golden_case_where_go_differs() {
    let g = golden::<JsonCase>("chunker_json");
    for (name, ledger, want) in DEVIATIONS {
        let cases: Vec<&JsonCase> = g.cases().iter().filter(|c| c.name == *name).collect();
        assert!(
            !cases.is_empty(),
            "{name} (ledger {ledger}) is not a golden case"
        );
        for c in cases {
            let differs = match want {
                Expected::Error(text) => c.err != *text,
                Expected::PredicateLang(p, _) => c.nquads.iter().all(|nq| nq.predicate != *p),
            };
            assert!(
                differs,
                "{name} (ledger {ledger}): Go gives the port's result"
            );
        }
    }
}
