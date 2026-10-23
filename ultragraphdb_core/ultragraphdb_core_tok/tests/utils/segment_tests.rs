/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{SegmentType, word_segments};

use crate::golden::{cases, untext};

#[derive(Debug, Clone, Deserialize)]
struct SegmentCase {
    input: String,
    segments: Vec<String>,
    types: Vec<u8>,
    error: String,
}

#[test]
fn test_word_segments_golden() {
    let cases: Vec<SegmentCase> = cases("tok_segment");
    assert!(cases.len() > 1900);
    let mut failures = Vec::new();
    for c in &cases {
        let input = untext(&c.input);
        let (segs, err) = word_segments(&input);
        let got: Vec<(String, u8)> = segs
            .iter()
            .map(|(s, t)| (s.to_string(), *t as u8))
            .collect();
        let want: Vec<(String, u8)> = c
            .segments
            .iter()
            .map(|s| untext(s))
            .zip(c.types.iter().copied())
            .collect();
        if got != want || err.unwrap_or("") != c.error {
            failures.push(format!("{input:?}: want {want:?}, got {got:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

#[test]
fn test_word_segments_types() {
    let (segs, err) = word_segments("Now  is the.\n End. 3.5 漢字 カタカナ 한국");
    assert!(err.is_none());
    let types: Vec<SegmentType> = segs.iter().map(|s| s.1).collect();
    assert_eq!(types[0], SegmentType::Letter);
    assert_eq!(types[1], SegmentType::None);
    assert!(types.contains(&SegmentType::Number));
    assert!(types.contains(&SegmentType::Ideo));
}

#[test]
fn test_word_segments_go_on_after_han() {
    // segment drops the rest of the input after a Han segment when an invalid byte follows (ledger 18); text holds
    // no invalid byte, and a replacement character is a segment of its own.
    let (segs, err) = word_segments("日\u{fffd} abc");
    assert!(err.is_none());
    assert_eq!(
        segs,
        vec![
            ("日", SegmentType::Ideo),
            ("\u{fffd}", SegmentType::None),
            (" ", SegmentType::None),
            ("abc", SegmentType::Letter)
        ]
    );
}
