/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{SegmentType, word_segments};

use crate::golden::{cases, unhex};

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
    assert!(cases.len() > 2000);
    let mut failures = Vec::new();
    for c in &cases {
        let input = unhex(&c.input);
        let (segs, err) = word_segments(&input);
        let got: Vec<(Vec<u8>, u8)> = segs.iter().map(|(b, t)| (b.to_vec(), *t as u8)).collect();
        let want: Vec<(Vec<u8>, u8)> = c
            .segments
            .iter()
            .map(|s| unhex(s))
            .zip(c.types.iter().copied())
            .collect();
        if got != want || err.unwrap_or("") != c.error {
            failures.push(format!(
                "{:?}: want {want:?}, got {got:?}",
                String::from_utf8_lossy(&input)
            ));
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
    let (segs, err) = word_segments("Now  is the.\n End. 3.5 漢字 カタカナ 한국".as_bytes());
    assert!(err.is_none());
    let types: Vec<SegmentType> = segs.iter().map(|s| s.1).collect();
    assert_eq!(types[0], SegmentType::Letter);
    assert_eq!(types[1], SegmentType::None);
    assert!(types.contains(&SegmentType::Number));
    assert!(types.contains(&SegmentType::Ideo));
}

#[test]
fn test_word_segments_stop_at_invalid_byte_after_han() {
    // segment stops when the rune after a Han segment is invalid, dropping the rest.
    let (segs, err) = word_segments(b"\xe6\x97\xa5\xff abc");
    assert!(err.is_none());
    assert!(segs.is_empty());
}
