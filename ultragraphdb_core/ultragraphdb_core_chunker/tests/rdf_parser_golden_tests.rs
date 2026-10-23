/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/chunker.go`: ParseRDF and ParseRDFs.

use ultragraphdb_core_chunker::{ChunkerErrorEnum, parse_rdf, parse_rdfs};
use ultragraphdb_core_lex::Lexer;

use crate::golden_nquad::{RdfCase, RdfsCase, golden, nquad_json, unhex};

/// The cases whose port result differs from Go's: the input, the entry of
/// `openspec/notes/dgraph-bugs-found-in-port.md`, and the port's error.
///
/// Entry 19: Dgraph's ObjectValue panics on a typed literal that converts to no geometry; the port refuses it as it
/// refuses a typed literal with no value.
const DEVIATIONS: &[(&str, u32, &str)] = &[(
    r#"<a> <p> "null"^^<geo:geojson> ."#,
    19,
    "invalid ObjectValue",
)];

fn check_rdf(c: &RdfCase) -> Result<(), String> {
    if let Some((_, _, want)) = DEVIATIONS.iter().find(|(input, _, _)| *input == c.input) {
        let got = parse_rdf(&c.input, &mut Lexer::new(&c.input));
        return match &got {
            Err(e) if e.to_string() == *want => Ok(()),
            _ => Err(format!("{:?}: deviation: {got:?}, want {want:?}", c.input)),
        };
    }
    if !c.valid_utf8 {
        // Go lexes each byte that is not UTF-8 as one rune of width 1. The port reads it as U+FFFD, which an IRI
        // keeps and Go's JSON writes in place of the raw byte in the golden file, and counts it as one column.
        let got = parse_rdfs(&unhex(&c.input_hex));
        return match (&c.nquad, got) {
            (Some(want), Ok((nqs, _))) if nqs.len() == 1 && *want == nquad_json(&nqs[0], false) => {
                Ok(())
            }
            (None, Ok((nqs, _))) if c.empty && nqs.is_empty() => Ok(()),
            (None, Err(e)) if !c.empty && e.to_string() == c.err => Ok(()),
            (_, got) => Err(format!(
                "{:?}: {got:?}, want {:?} / {:?}",
                c.input, c.nquad, c.err
            )),
        };
    }
    let got = parse_rdf(&c.input, &mut Lexer::new(&c.input));
    match (&c.nquad, &got) {
        (Some(want), Ok(nq)) if *want == nquad_json(nq, false) => Ok(()),
        (None, Err(e)) if c.empty && e.0 == ChunkerErrorEnum::Empty => Ok(()),
        (None, Err(e)) if !c.empty && e.to_string() == c.err => Ok(()),
        _ => Err(format!(
            "{:?}: {got:?}, want {:?} / {:?}",
            c.input, c.nquad, c.err
        )),
    }
}

#[test]
fn parse_rdf_matches_go() {
    let g = golden::<RdfCase>("chunker_rdf");
    assert!(g.cases().len() > 300);
    assert_eq!(g.cases().iter().filter(|c| c.from_table).count(), 130);
    let failures: Vec<String> = g
        .cases()
        .iter()
        .filter_map(|c| check_rdf(c).err())
        .collect();
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_deviation_names_a_golden_case_where_go_panics() {
    let g = golden::<RdfCase>("chunker_rdf");
    for (input, ledger, _) in DEVIATIONS {
        let c = g.cases().iter().find(|c| c.input == *input);
        assert!(
            c.is_some_and(|c| !c.panic.is_empty()),
            "{input} (ledger {ledger}): no golden case where Go panics"
        );
    }
}

#[test]
fn the_table_expectations_are_what_go_parses() {
    for c in golden::<RdfCase>("chunker_rdf")
        .cases()
        .iter()
        .filter(|c| c.from_table)
    {
        assert_eq!(c.table_expected_err, c.nquad.is_none(), "{:?}", c.input);
        assert_eq!(c.table_should_ignore, c.empty, "{:?}", c.input);
    }
}

#[test]
fn parse_rdfs_matches_go() {
    for c in golden::<RdfsCase>("chunker_rdfs").cases() {
        let got = parse_rdfs(&unhex(&c.input));
        match got {
            Ok((nqs, md)) => {
                assert!(c.err.is_empty(), "{}: {:?}", c.name, c.err);
                let nqs: Vec<_> = nqs.iter().map(|nq| nquad_json(nq, false)).collect();
                assert_eq!(nqs, c.nquads, "{}", c.name);
                let mut hints: Vec<(String, i32)> = md.pred_hints.into_iter().collect();
                hints.sort();
                let want: Vec<(String, i32)> =
                    c.hints.iter().map(|h| (h.pred.clone(), h.hint)).collect();
                assert_eq!(hints, want, "{}", c.name);
            }
            Err(e) => assert_eq!(e.to_string(), c.err, "{}", c.name),
        }
    }
}
