/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The trees clone block by block and node by node without recursion (ledger 32); a clone equals its original.

use ultragraphdb_core_dql::parse;

use crate::parser::golden_parse::{golden, request};

/// Every tree of the golden corpus, its filters and math expressions included, equals its clone in every field.
#[test]
fn test_every_golden_tree_equals_its_clone() {
    let mut cloned = 0;
    for c in golden().cases() {
        if let Ok(res) = parse(&request(c)) {
            for gq in res.query() {
                assert_eq!(gq.clone(), *gq, "{:?}", c.input);
                for tree in [gq.filter(), gq.facets_filter()].into_iter().flatten() {
                    assert_eq!(tree.clone(), *tree, "{:?}", c.input);
                }
            }
            cloned += 1;
        }
    }
    assert!(cloned > 500, "only {cloned} trees cloned");
}
