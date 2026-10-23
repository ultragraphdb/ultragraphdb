/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{AnalysisToken, analyze};

use crate::golden::{cases, unhex};

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
    pub(crate) input: String,
    pub(crate) tokens: Vec<StreamToken>,
}

/// A token stream as (term, start, end, type).
pub(crate) fn stream(tokens: &[AnalysisToken]) -> Vec<(Vec<u8>, usize, usize, u8)> {
    tokens
        .iter()
        .map(|t| (t.term().to_vec(), t.start(), t.end(), t.typ() as u8))
        .collect()
}

pub(crate) fn want(c: &StreamCase) -> Vec<(Vec<u8>, usize, usize, u8)> {
    c.tokens
        .iter()
        .map(|t| (unhex(&t.term), t.start, t.end, t.typ))
        .collect()
}

#[test]
fn test_analyze_golden() {
    let cases: Vec<StreamCase> = cases("tok_analysis");
    assert!(cases.len() > 2000);
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|c| {
            let input = unhex(&c.input);
            let got = stream(&analyze(&input));
            (got != want(c)).then(|| {
                format!(
                    "{:?}: want {:?}, got {got:?}",
                    String::from_utf8_lossy(&input),
                    want(c)
                )
            })
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

#[test]
fn test_analyze_lowercases_and_normalizes() {
    let terms: Vec<Vec<u8>> = analyze("ΟΔΟΣ ﬁnancial ＡＢＣ".as_bytes())
        .iter()
        .map(|t| t.term().to_vec())
        .collect();
    assert_eq!(
        terms,
        vec![
            "οδος".as_bytes().to_vec(),
            b"financial".to_vec(),
            b"abc".to_vec()
        ]
    );
}
