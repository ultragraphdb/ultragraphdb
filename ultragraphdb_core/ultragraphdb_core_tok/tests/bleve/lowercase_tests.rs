/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/token/lowercase/lowercase_test.go (TestLowerCaseFilter). Changed for
 * ultragraphdb: translated to Rust; the filter is reached through `analyze`, on terms the tokenizer keeps whole, with
 * the final-sigma rule's edges added from toLowerDeferredCopy and every expected term bleve's analyzer's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::analyze;

fn terms(input: &str) -> Vec<String> {
    analyze(input)
        .iter()
        .map(|t| t.term().to_string())
        .collect()
}

#[test]
fn test_lower_case_filter() {
    let tests = [
        // bleve's table.
        ("ONE", "one"),
        ("two", "two"),
        ("ThReE", "three"),
        ("steven's", "steven's"),
        ("İȺȾCAT", "iⱥⱦcat"),
        ("ȺȾCAT", "ⱥⱦcat"),
        ("ὈΔΥΣΣ", "ὀδυσς"),
        // A Σ ending the term, alone too, is ς; one inside it is σ.
        ("Σ", "ς"),
        ("ΑΣ", "ας"),
        ("ΣΑ", "σα"),
        ("ΣΣΣ", "σσς"),
        // A rune already lower stays; a titlecase digraph lowercases to ǆ, which NFKC then splits into dž.
        ("ß", "ß"),
        ("ǅ", "dž"),
    ];
    for (input, want) in tests {
        assert_eq!(terms(input), vec![want], "{input:?}");
    }
}

#[test]
fn test_lower_case_filter_stream() {
    // bleve's second case: every term of a stream is lowercased.
    assert_eq!(
        terms("A boiling liquid EXPANDING Vapor"),
        vec!["a", "boiling", "liquid", "expanding", "vapor"]
    );
}
