/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{AnalysisToken, TokenType, analyze};

use crate::golden::{LOWERCASE_DEVIATIONS, cases, unhex, untext};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StreamToken {
    term: String,
    start: usize,
    end: usize,
    #[serde(rename = "type")]
    typ: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StreamCase {
    input: String,
    tokens: Vec<StreamToken>,
    /// The port's stream, where it differs from Dgraph's.
    #[serde(default)]
    fixed: Option<Vec<StreamToken>>,
}

/// A token stream as (term, start, end, type); Dgraph's terms need not be UTF-8 (ledger 16).
type Stream = Vec<(Vec<u8>, usize, usize, u8)>;

pub(crate) fn stream(tokens: &[AnalysisToken]) -> Stream {
    tokens
        .iter()
        .map(|t| {
            (
                t.term().as_bytes().to_vec(),
                t.start(),
                t.end(),
                t.typ() as u8,
            )
        })
        .collect()
}

fn decoded(tokens: &[StreamToken]) -> Stream {
    tokens
        .iter()
        .map(|t| (unhex(&t.term), t.start, t.end, t.typ))
        .collect()
}

/// Compares the port's stream of each golden case of `name`, by `run`, with Dgraph's, and with the case's `fixed`
/// stream for the inputs [`LOWERCASE_DEVIATIONS`] names, on which Dgraph's differs.
pub(crate) fn check_streams(name: &str, run: impl Fn(&str) -> Vec<AnalysisToken>) {
    let cases: Vec<StreamCase> = cases(name);
    let mut deviating = 0;
    let mut failures = Vec::new();
    for c in &cases {
        let input = untext(&c.input);
        let deviation = LOWERCASE_DEVIATIONS.iter().find(|(i, _)| *i == input);
        let want = match (&c.fixed, deviation) {
            (Some(fixed), Some(_)) => {
                deviating += 1;
                assert_ne!(
                    decoded(fixed),
                    decoded(&c.tokens),
                    "{input:?}: Dgraph differs"
                );
                decoded(fixed)
            }
            (None, None) => decoded(&c.tokens),
            (fixed, _) => panic!("{input:?}: fixed {fixed:?}, listed {deviation:?}"),
        };
        let got = stream(&run(&input));
        if got != want {
            failures.push(format!("{input:?}: want {want:?}, got {got:?}"));
        }
    }
    assert_eq!(deviating, LOWERCASE_DEVIATIONS.len(), "{name}");
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

#[test]
fn test_analyze_golden() {
    assert!(cases::<StreamCase>("tok_analysis").len() > 1900);
    check_streams("tok_analysis", analyze);
}

#[test]
fn test_analyze_lowercases_and_normalizes() {
    let terms: Vec<String> = analyze("ΟΔΟΣ ﬁnancial ＡＢＣ")
        .iter()
        .map(|t| t.term().to_string())
        .collect();
    assert_eq!(terms, vec!["οδος", "financial", "abc"]);
}

#[test]
fn test_analyze_keeps_every_rune_after_a_shorter_lowercase() {
    // İ (two bytes) lowercases to i (one); bleve leaves the bytes after it stale, `İб` as 69 B0 D0 (ledger 16).
    let terms: Vec<String> = analyze("İб İstanbul")
        .iter()
        .map(|t| t.term().to_string())
        .collect();
    assert_eq!(terms, vec!["iб", "istanbul"]);
}

#[test]
fn test_analyze_final_sigma() {
    // bleve's own case, and a final Σ after a rune whose lowercase form is wider (ledger 30).
    let terms: Vec<String> = analyze("ὈΔΥΣΣ ȺΣ AΣ")
        .iter()
        .map(|t| t.term().to_string())
        .collect();
    assert_eq!(terms, vec!["ὀδυσς", "ⱥς", "aς"]);
}

#[test]
fn test_analyze_keeps_words_and_their_offsets() {
    // No word segment, no token.
    for input in ["", " ", "\t\n", "...!?", "  -- "] {
        assert!(analyze(input).is_empty(), "{input:?}");
    }
    // Offsets are bytes of the input; the type follows the segment: letters alphanumeric, 3.5 numeric, Han
    // ideographic.
    assert_eq!(
        analyze("Ünï 3.5 漢"),
        vec![
            AnalysisToken::new("ünï", 0, 5, TokenType::AlphaNumeric),
            AnalysisToken::new("3.5", 6, 9, TokenType::Numeric),
            AnalysisToken::new("漢", 10, 13, TokenType::Ideographic),
        ]
    );
}
