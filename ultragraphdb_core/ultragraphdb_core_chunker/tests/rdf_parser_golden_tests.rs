/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/chunker.go`: ParseRDF and ParseRDFs.

use ultragraphdb_core_chunker::{ChunkerErrorEnum, parse_rdf, parse_rdfs};
use ultragraphdb_core_lex::Lexer;

use crate::golden_nquad::{RdfCase, RdfsCase, golden, nquad_json, unhex};

fn check_rdf(c: &RdfCase) -> Result<(), String> {
    if !c.valid_utf8 {
        // Go lexes each byte that is not UTF-8 as one rune of width 1; the port replaces it by U+FFFD (three
        // bytes) first, so the N-Quad is the same as Go's, written as JSON, and an error is an error.
        let got = parse_rdfs(&unhex(&c.input_hex));
        return match (&c.nquad, got) {
            (Some(want), Ok((nqs, _))) if nqs.len() == 1 && *want == nquad_json(&nqs[0], false) => {
                Ok(())
            }
            (None, Ok((nqs, _))) if c.empty && nqs.is_empty() => Ok(()),
            (None, Err(_)) if !c.empty => Ok(()),
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
        // Dgraph panics on `"null"^^<geo:geojson>`; the port refuses the line.
        (None, Err(_)) if !c.panic.is_empty() => Ok(()),
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
