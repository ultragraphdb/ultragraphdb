/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `Debug` of the trees, written without recursion, keeps the layout `#[derive(Debug)]` gives (ledger 32).

use ultragraphdb_core_dql::{Request, parse};

#[test]
fn test_debug_keeps_the_derived_layout() {
    let res = parse(&Request::new(
        "{ q(func: uid(1)) @filter(not has(a)) { a } }",
    ))
    .expect("parses");
    let filter = res.query()[0].filter().expect("a filter");
    assert_eq!(
        format!("{filter:?}"),
        r#"FilterTree { op: "not", child: [FilterTree { op: "", child: [], func: Some(Function { attr: "a", lang: "", name: "has", args: [], uid: [], needs_var: [], is_count: false, is_value_var: false, is_len_var: false }) }], func: None }"#
    );
    assert_eq!(
        format!("{filter:#?}"),
        r#"FilterTree {
    op: "not",
    child: [
        FilterTree {
            op: "",
            child: [],
            func: Some(
                Function {
                    attr: "a",
                    lang: "",
                    name: "has",
                    args: [],
                    uid: [],
                    needs_var: [],
                    is_count: false,
                    is_value_var: false,
                    is_len_var: false,
                },
            ),
        },
    ],
    func: None,
}"#
    );
    let block = format!("{:?}", res.query()[0]);
    assert!(
        block.starts_with("GraphQuery { uid: [1], attr: \"\", "),
        "{block}"
    );
    assert!(
        block.contains("children: [GraphQuery { uid: [], attr: \"a\", "),
        "{block}"
    );
    assert!(block.ends_with("is_empty: false }"), "{block}");
}
