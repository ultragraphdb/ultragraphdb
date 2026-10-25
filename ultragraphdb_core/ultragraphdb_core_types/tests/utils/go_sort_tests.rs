/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go 1.27's `sort.Sort` order of equal keys, which Dgraph's result order takes, against the golden vectors of
//! tools/golden/types_sort_order.go. `go_sort` is reached through `sort`, the crate's entry point: one int column
//! sorted ascending compares as Go's `Less` over the bare keys, and the uids, each a row's input index, give the
//! permutation. The heap tables reach pdqsort's heapSort fallback, as do the descending int and float columns with nil
//! values at 257 rows, and eight of their eighteen inputs at 100 rows.

use serde::Deserialize;
use ultragraphdb_core_types::{TextCollator, TypeId, Val, Value, sort};

#[derive(Deserialize)]
struct SortOrderCase {
    name: String,
    layer: String,
    #[serde(default)]
    tid: String,
    #[serde(default)]
    desc: bool,
    keys: Vec<Option<i64>>,
    #[serde(default)]
    heap_table: bool,
    perm: Vec<u64>,
}

/// The value a golden key stands for: an int, half the key as a float, or its decimal text; `None` a nil value.
fn val(tid: &str, key: Option<i64>) -> Val {
    let t = match tid {
        "int" => TypeId::INT,
        "float" => TypeId::FLOAT,
        "string" => TypeId::STRING,
        other => panic!("golden tid {other:?}"),
    };
    match key {
        None => Val::nil(t),
        Some(k) => Val::new(
            t,
            match tid {
                "float" => Value::Float(k as f64 / 2.0),
                "string" => Value::str(k.to_string()),
                _ => Value::Int(k),
            },
        ),
    }
}

#[test]
fn test_golden_sort_order_of_ties() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_sort_order.json");
    let golden = ultragraphdb_test_bdd::load_golden::<SortOrderCase>(&path);
    let cases = golden.cases();
    let count = |layer: &str, heap: bool| {
        cases
            .iter()
            .filter(|c| c.layer == layer && c.heap_table == heap)
            .count()
    };
    assert_eq!(count("sort", false), 104);
    assert_eq!(count("sort", true), 18);
    assert_eq!(count("types", false), 360);
    assert_eq!(count("types", true), 54);
    assert_eq!(cases.len(), 536);
    for c in cases {
        let tid = match c.layer.as_str() {
            "sort" => {
                assert!(c.keys.iter().all(Option::is_some), "{}", c.name);
                assert!(!c.desc, "{}", c.name);
                "int"
            }
            "types" => c.tid.as_str(),
            other => panic!("golden layer {other:?}"),
        };
        let mut rows: Vec<Vec<Val>> = c.keys.iter().map(|k| vec![val(tid, *k)]).collect();
        let mut uids: Vec<u64> = (0..c.keys.len() as u64).collect();
        sort(&mut rows, &mut uids, &[c.desc], "", &TextCollator).unwrap();
        assert_eq!(uids, c.perm, "{}", c.name);
        let moved: Vec<Vec<Val>> = c
            .perm
            .iter()
            .map(|&i| vec![val(tid, c.keys[i as usize])])
            .collect();
        assert_eq!(rows, moved, "{}: rows move with their uids", c.name);
    }
}
