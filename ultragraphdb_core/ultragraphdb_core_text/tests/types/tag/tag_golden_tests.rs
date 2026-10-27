/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_text::{Confidence, Tag};
use ultragraphdb_tool_bdd::{load_golden, testdata_dir};

#[derive(Debug, Deserialize)]
struct LangBaseCase {
    tag: String,
    parse_error: String,
    string: String,
    is_und: bool,
    base: String,
    confidence: String,
}

fn confidence(c: Confidence) -> &'static str {
    match c {
        Confidence::No => "No",
        Confidence::Low => "Low",
        Confidence::High => "High",
        Confidence::Exact => "Exact",
    }
}

/// Every tag parses, prints, compares with `und` and infers its base exactly as `golang.org/x/text/language`.
#[test]
fn tags_match_go() {
    check_file("text_lang_base", 900);
}

/// The same for tags drawn at random from subtags and separators.
#[test]
fn random_tags_match_go() {
    check_file("text_lang_random", 2500);
}

fn check_file(name: &str, min_cases: usize) {
    let path = testdata_dir().join(format!("golden/{name}.json"));
    let golden = load_golden::<LangBaseCase>(&path);
    assert!(golden.cases().len() > min_cases);
    let mut failures = Vec::new();
    for c in golden.cases() {
        let got = match Tag::parse(&c.tag) {
            Err(e) => (
                e.to_string(),
                String::new(),
                false,
                String::new(),
                String::new(),
            ),
            Ok(t) => {
                let (b, conf) = t.base();
                (
                    String::new(),
                    t.to_string(),
                    t == Tag::default(),
                    b.to_string(),
                    confidence(conf).to_string(),
                )
            }
        };
        let want = (
            c.parse_error.clone(),
            c.string.clone(),
            c.is_und,
            c.base.clone(),
            c.confidence.clone(),
        );
        if got != want {
            failures.push(format!("{:?}: got {got:?}, want {want:?}", c.tag));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        golden.cases().len(),
        failures.join("\n")
    );
}
