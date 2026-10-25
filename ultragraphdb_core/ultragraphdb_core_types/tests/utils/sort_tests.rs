/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/sort_test.go. Changed for ultragraphdb: translated to Rust; plus the golden sort
 * orders (tools/golden/types.go, types_sort and types_sort_lang).
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_types::{
    TextCollator, TypeId, Val, Value, convert, equal, is_sortable, less, marshal, sort, sort_top_n,
    sort_with_facet,
};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb;

fn to_string(values: &[Vec<Val>]) -> Vec<String> {
    values
        .iter()
        .map(
            |v| match marshal(&v[0], TypeId::STRING).unwrap().into_value() {
                Some(Value::Str(s)) => s,
                other => panic!("{other:?}"),
            },
        )
        .collect()
}

fn get_input(tid: TypeId, input: &[&str]) -> Vec<Vec<Val>> {
    input
        .iter()
        .map(|s| vec![convert(&Val::bytes(TypeId::STRING, s.as_bytes()), tid).unwrap()])
        .collect()
}

fn get_uid_list(n: usize) -> Vec<u64> {
    (1..=n as u64).map(|i| i * 100).collect()
}

#[test]
fn test_quick_select() {
    let (n, k) = (10000, 10);
    let mut seed: u64 = 7;
    let mut list: Vec<Vec<Val>> = (0..n)
        .map(|_| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let s = ((seed >> 33) % 100000).to_string();
            vec![convert(&Val::bytes(TypeId::STRING, s.into_bytes()), TypeId::INT).unwrap()]
        })
        .collect();
    let mut ul = get_uid_list(n);
    sort_top_n(&mut list, &mut ul, &[false], "", k, &TextCollator).unwrap();
    let int = |v: &Val| match v.value() {
        Some(Value::Int(i)) => *i,
        _ => panic!(),
    };
    for i in 0..k {
        for j in k..n {
            assert!(int(&list[i][0]) <= int(&list[j][0]));
        }
    }
    // SortTopN also sorts the first k, and the uids move with their values.
    for w in list[..k].windows(2) {
        assert!(int(&w[0][0]) <= int(&w[1][0]));
    }
    let mut seed: u64 = 7;
    let original: Vec<i64> = (0..n)
        .map(|_| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) % 100000) as i64
        })
        .collect();
    for (row, uid) in list.iter().zip(&ul) {
        assert_eq!(
            int(&row[0]),
            original[(*uid / 100 - 1) as usize],
            "uid {uid}"
        );
    }
}

/// One-column int rows, `None` a nil value, with uids 100, 200, ...
fn int_rows(input: &[Option<i64>]) -> (Vec<Vec<Val>>, Vec<u64>) {
    let rows = input
        .iter()
        .map(|v| {
            vec![v.map_or(Val::nil(TypeId::INT), |i| {
                Val::new(TypeId::INT, Value::Int(i))
            })]
        })
        .collect();
    (rows, get_uid_list(input.len()))
}

/// The rows' values after a sort, `None` a nil value.
fn int_values(rows: &[Vec<Val>]) -> Vec<Option<i64>> {
    rows.iter()
        .map(|r| match r[0].value() {
            Some(Value::Int(i)) => Some(*i),
            None => None,
            other => panic!("{other:?}"),
        })
        .collect()
}

/// Runs `sort_top_n` over `input` for each `n` and checks the uids and values against one expected order.
fn assert_top_n(
    input: &[Option<i64>],
    desc: bool,
    ns: &[usize],
    uids: &[u64],
    values: &[Option<i64>],
) {
    for &n in ns {
        let (mut rows, mut ul) = int_rows(input);
        sort_top_n(&mut rows, &mut ul, &[desc], "", n, &TextCollator).unwrap();
        assert_eq!(ul, uids, "uids, desc {desc}, n {n}");
        assert_eq!(int_values(&rows), values, "values, desc {desc}, n {n}");
    }
}

// The expected orders for n up to the row count are Go 1.27's types.SortTopN over the same rows (Dgraph
// v25 types/sort.go:95); past the row count Go panics with `slice bounds out of range [:n] with capacity
// len`, and the port returns the order of n equal to the row count, as SQL's LIMIT and Dgraph's first: do.

#[test]
fn test_sort_top_n_with_nil_values_and_ties() {
    let input = [
        Some(5),
        Some(3),
        None,
        Some(3),
        Some(1),
        Some(5),
        None,
        Some(2),
    ];
    let asc = [
        Some(1),
        Some(2),
        Some(3),
        Some(3),
        Some(5),
        Some(5),
        None,
        None,
    ];
    assert_top_n(
        &input,
        false,
        &[0, 1, 2, 3, 5, 6, 7, 8, 9, 100, usize::MAX],
        &[500, 800, 200, 400, 100, 600, 700, 300],
        &asc,
    );
    let desc = [
        Some(5),
        Some(5),
        Some(3),
        Some(3),
        Some(2),
        Some(1),
        None,
        None,
    ];
    assert_top_n(
        &input,
        true,
        &[0, 2, 3, 6],
        &[100, 600, 200, 400, 800, 500, 700, 300],
        &desc,
    );
    // Sorting all eight rows swaps the two nil rows: Dgraph's Less of two nil values is the direction.
    assert_top_n(
        &input,
        true,
        &[8, 9, usize::MAX],
        &[100, 600, 200, 400, 800, 500, 300, 700],
        &desc,
    );
}

#[test]
fn test_sort_top_n_counts_past_the_rows_sort_every_row() {
    // Twelve values and a nil: n of 13 sorts the nil row too, and Go's pdqsort then orders the ties
    // differently from the insertion sort quickselect ends in.
    let input = [
        Some(7),
        Some(3),
        Some(9),
        Some(3),
        Some(11),
        Some(0),
        Some(-4),
        Some(8),
        Some(3),
        Some(12),
        Some(-4),
        Some(6),
        None,
    ];
    let values = [
        Some(-4),
        Some(-4),
        Some(0),
        Some(3),
        Some(3),
        Some(3),
        Some(6),
        Some(7),
        Some(8),
        Some(9),
        Some(11),
        Some(12),
        None,
    ];
    assert_top_n(
        &input,
        false,
        &[0, 1, 4, 5, 12],
        &[
            700, 1100, 600, 200, 400, 900, 1200, 100, 800, 300, 500, 1000, 1300,
        ],
        &values,
    );
    assert_top_n(
        &input,
        false,
        &[13, 14, usize::MAX],
        &[
            1100, 700, 600, 400, 900, 200, 1200, 100, 800, 300, 500, 1000, 1300,
        ],
        &values,
    );
}

#[test]
fn test_sort_top_n_leaves_the_rest_selected_not_sorted() {
    let input: Vec<Option<i64>> = [
        14, 2, 27, 2, 9, 30, 5, 18, 2, 21, 11, 0, 25, 7, 16, 9, 3, 29, 12, 1, 23, 9, 6, 19, 8, 28,
        4, 15, 10, 22,
    ]
    .into_iter()
    .map(Some)
    .collect();
    let some = |v: &[i64]| v.iter().copied().map(Some).collect::<Vec<_>>();
    assert_top_n(
        &input,
        false,
        &[0, 1, 3, 4],
        &[
            1200, 2000, 200, 400, 900, 1700, 2700, 700, 1400, 1600, 2900, 1900, 100, 2200, 2300,
            2500, 1100, 2800, 500, 1500, 2100, 600, 3000, 2400, 1000, 2600, 300, 1800, 800, 1300,
        ],
        &some(&[
            0, 1, 2, 2, 2, 3, 4, 5, 7, 9, 10, 12, 14, 9, 6, 8, 11, 15, 9, 16, 23, 30, 22, 19, 21,
            28, 27, 29, 18, 25,
        ]),
    );
    assert_top_n(
        &input,
        false,
        &[10],
        &[
            1200, 2000, 200, 400, 900, 1700, 2700, 700, 2300, 1400, 2500, 1600, 500, 2200, 2900,
            1900, 1100, 2800, 100, 1500, 2100, 600, 3000, 2400, 1000, 2600, 300, 1800, 800, 1300,
        ],
        &some(&[
            0, 1, 2, 2, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 10, 12, 11, 15, 14, 16, 23, 30, 22, 19, 21,
            28, 27, 29, 18, 25,
        ]),
    );
    let sorted = some(&[
        0, 1, 2, 2, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 10, 11, 12, 14, 15, 16, 18, 19, 21, 22, 23, 25,
        27, 28, 29, 30,
    ]);
    assert_top_n(
        &input,
        false,
        &[29],
        &[
            1200, 2000, 200, 400, 900, 1700, 2700, 700, 2300, 1400, 2500, 1600, 2200, 500, 2900,
            1100, 1900, 100, 2800, 1500, 800, 2400, 1000, 3000, 2100, 1300, 300, 2600, 1800, 600,
        ],
        &sorted,
    );
    assert_top_n(
        &input,
        false,
        &[30, 31, usize::MAX],
        &[
            1200, 2000, 900, 200, 400, 1700, 2700, 700, 2300, 1400, 2500, 500, 1600, 2200, 2900,
            1100, 1900, 100, 2800, 1500, 800, 2400, 1000, 3000, 2100, 1300, 300, 2600, 1800, 600,
        ],
        &sorted,
    );
    assert_top_n(
        &input,
        true,
        &[3],
        &[
            600, 1800, 2600, 300, 1300, 3000, 1000, 2100, 2400, 800, 1500, 1200, 500, 1400, 200,
            1600, 1700, 700, 1900, 2000, 100, 2200, 2300, 900, 2500, 400, 2700, 2800, 2900, 1100,
        ],
        &some(&[
            30, 29, 28, 27, 25, 22, 21, 23, 19, 18, 16, 0, 9, 7, 2, 9, 3, 5, 12, 1, 14, 9, 6, 2, 8,
            2, 4, 15, 10, 11,
        ]),
    );
    assert_top_n(
        &input,
        true,
        &[30, 31, usize::MAX],
        &[
            600, 1800, 2600, 300, 1300, 2100, 3000, 1000, 2400, 800, 1500, 2800, 100, 1900, 1100,
            2900, 500, 1600, 2200, 2500, 1400, 2300, 700, 2700, 1700, 400, 900, 200, 2000, 1200,
        ],
        &some(&[
            30, 29, 28, 27, 25, 23, 22, 21, 19, 18, 16, 15, 14, 12, 11, 10, 9, 9, 9, 8, 7, 6, 5, 4,
            3, 2, 2, 2, 1, 0,
        ]),
    );
}

#[test]
fn test_sort_top_n_of_no_rows() {
    for n in [0, 1, 5, usize::MAX] {
        let (mut rows, mut ul) = int_rows(&[]);
        sort_top_n(&mut rows, &mut ul, &[false], "", n, &TextCollator).unwrap();
        assert!(rows.is_empty(), "n {n}");
        assert!(ul.is_empty(), "n {n}");
    }
}

#[test]
fn test_sort_strings() {
    let mut list = get_input(TypeId::STRING, &["bb", "aaa", "aa", "bab"]);
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator).unwrap();
    assert_eq!(ul, [300, 200, 400, 100]);
    assert_eq!(to_string(&list), ["aa", "aaa", "bab", "bb"]);
}

#[test]
fn test_sort_language() {
    let mut list = get_input(TypeId::STRING, &["öffnen", "zumachen"]);
    let mut ul = get_uid_list(2);
    sort(&mut list, &mut ul, &[false], "de", &TextCollator).unwrap();
    assert_eq!(ul, [100, 200]);
    assert_eq!(to_string(&list), ["öffnen", "zumachen"]);

    let mut list = get_input(TypeId::STRING, &["öppna", "zon"]);
    sort(&mut list, &mut ul, &[false], "sv", &TextCollator).unwrap();
    assert_eq!(ul, [200, 100]);
    assert_eq!(to_string(&list), ["zon", "öppna"]);
}

#[test]
fn test_sort_ints() {
    let mut list = get_input(TypeId::INT, &["22", "111", "11", "212"]);
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator).unwrap();
    assert_eq!(ul, [300, 100, 200, 400]);
    assert_eq!(to_string(&list), ["11", "22", "111", "212"]);
}

#[test]
fn test_sort_floats() {
    let mut list = get_input(TypeId::FLOAT, &["22.2", "11.2", "11.5", "2.12"]);
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator).unwrap();
    assert_eq!(ul, [400, 200, 300, 100]);
    assert_eq!(to_string(&list), ["2.12", "11.2", "11.5", "22.2"]);
}

#[test]
fn test_sort_floats_desc() {
    let mut list = get_input(TypeId::FLOAT, &["22.2", "11.2", "11.5", "2.12"]);
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[true], "", &TextCollator).unwrap();
    assert_eq!(ul, [100, 300, 200, 400]);
    assert_eq!(to_string(&list), ["22.2", "11.5", "11.2", "2.12"]);
}

#[test]
fn test_sort_date_times() {
    let input = [
        "2016-01-02T15:04:05",
        "2006-01-02T15:04:05",
        "2006-01-02T15:04:06",
        "2006-01-02T15:04:01",
    ];
    let mut list = get_input(TypeId::DATETIME, &input);
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator).unwrap();
    assert_eq!(ul, [400, 200, 300, 100]);
    assert_eq!(
        to_string(&list),
        [
            "2006-01-02T15:04:01Z",
            "2006-01-02T15:04:05Z",
            "2006-01-02T15:04:06Z",
            "2016-01-02T15:04:05Z"
        ]
    );
}

#[test]
fn test_sort_int_and_float() {
    let mut list = vec![
        vec![Val::new(TypeId::INT, Value::Int(55))],
        vec![Val::new(TypeId::FLOAT, Value::Float(21.5))],
        vec![Val::new(TypeId::INT, Value::Int(100))],
    ];
    let mut ul = get_uid_list(3);
    sort(&mut list, &mut ul, &[false], "", &TextCollator).unwrap();
    assert_eq!(ul, [200, 100, 300]);
    assert_eq!(to_string(&list), ["21.5", "55", "100"]);
}

#[test]
fn test_equal() {
    let int = |i| Val::new(TypeId::INT, Value::Int(i));
    assert!(equal(&int(3), &int(3)).unwrap());
    assert!(!equal(&int(3), &int(4)).unwrap());
    assert_eq!(
        equal(&int(3), &Val::new(TypeId::FLOAT, Value::Float(3.0)))
            .unwrap_err()
            .to_string(),
        "Arguments of different type can not be compared."
    );
    assert!(!equal(&int(3), &Val::nil(TypeId::INT)).unwrap());
    assert!(!equal(&Val::nil(TypeId::INT), &int(3)).unwrap());
    assert!(!equal(&Val::nil(TypeId::INT), &Val::nil(TypeId::INT)).unwrap());
    let int_with_float = Val::new(TypeId::INT, Value::Float(3.0));
    assert!(!equal(&int_with_float, &Val::new(TypeId::INT, Value::Float(3.0))).unwrap());
    assert!(
        !equal(
            &Val::new(TypeId::FLOAT, Value::Float(3.0)),
            &Val::new(TypeId::FLOAT, Value::Int(3))
        )
        .unwrap()
    );
}

fn find_index(uids: &[u64], uid: u64) -> usize {
    uids.iter()
        .position(|u| *u == uid)
        .expect("Could not find index")
}

#[test]
fn test_sort_mismatched_types() {
    let mut list = vec![
        vec![Val::new(TypeId::STRING, Value::str("cat"))],
        vec![Val::new(TypeId::INT, Value::Int(55))],
        vec![Val::new(TypeId::BOOL, Value::Bool(true))],
        vec![Val::new(TypeId::FLOAT, Value::Float(21.5))],
        vec![Val::new(TypeId::STRING, Value::str("aardvark"))],
        vec![Val::new(TypeId::STRING, Value::str("buffalo"))],
        vec![Val::new(TypeId::FLOAT, Value::Float(33.33))],
    ];
    let mut ul = get_uid_list(7);
    sort(&mut list, &mut ul, &[false], "", &TextCollator).unwrap();
    assert!(find_index(&ul, 500) < find_index(&ul, 600));
    assert!(find_index(&ul, 600) < find_index(&ul, 100));
    assert!(find_index(&ul, 400) < find_index(&ul, 700));
    assert!(find_index(&ul, 700) < find_index(&ul, 200));
}

#[test]
fn test_less_errors() {
    let int = Val::new(TypeId::INT, Value::Int(1));
    assert_eq!(
        less(&int, &Val::new(TypeId::FLOAT, Value::Float(1.0)))
            .unwrap_err()
            .to_string(),
        "Arguments of different type can not be compared."
    );
    let b = Val::new(TypeId::BOOL, Value::Bool(true));
    assert_eq!(
        less(&b, &b).unwrap_err().to_string(),
        "Compare not supported for type: 4"
    );
    let g = Val::new(TypeId::GEO, Value::Bytes(Vec::new()));
    assert_eq!(
        equal(&g, &g).unwrap_err().to_string(),
        "Equal not supported for type: 6"
    );
}

#[derive(Deserialize)]
struct SortVal {
    tid: i32,
    #[serde(default)]
    bin: String,
    #[serde(default)]
    nil: bool,
}

#[derive(Deserialize)]
struct SortCase {
    name: String,
    rows: Option<Vec<Vec<SortVal>>>,
    desc: Vec<bool>,
    #[serde(default)]
    top_n: usize,
    #[serde(default)]
    uids: Option<Vec<u64>>,
    #[serde(default)]
    err: String,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn test_golden_sort_orders() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_sort.json");
    let golden = ultragraphdb_test_bdd::load_golden::<SortCase>(&path);
    for c in golden.cases() {
        let mut rows: Vec<Vec<Val>> = c
            .rows
            .iter()
            .flatten()
            .map(|r| {
                r.iter()
                    .map(|v| {
                        let tid = TypeId::new(v.tid);
                        if v.nil {
                            Val::nil(tid)
                        } else if tid == TypeId::UID {
                            let b = unhex(&v.bin);
                            Val::new(
                                tid,
                                Value::Uid(u64::from_le_bytes(b[..8].try_into().unwrap())),
                            )
                        } else {
                            convert(&Val::bytes(TypeId::BINARY, unhex(&v.bin)), tid).unwrap()
                        }
                    })
                    .collect()
            })
            .collect();
        let mut uids = get_uid_list(rows.len());
        let res = if c.top_n > 0 {
            sort_top_n(&mut rows, &mut uids, &c.desc, "", c.top_n, &TextCollator)
        } else {
            sort(&mut rows, &mut uids, &c.desc, "", &TextCollator)
        };
        match res {
            Err(e) => assert_eq!(e.to_string(), c.err, "{}", c.name),
            Ok(()) => assert_eq!(uids, c.uids.clone().unwrap_or_default(), "{}", c.name),
        }
    }
}

/// A language sort case; the strings, in hex, may hold invalid UTF-8.
#[derive(Debug, Clone, Deserialize)]
struct LangSortCase {
    lang: String,
    uids: Vec<u64>,
    in_hex: Vec<String>,
    out_hex: Vec<String>,
}

/// The golden language sorts whose strings hold invalid UTF-8, by language, with the ledger entries of
/// `openspec/notes/dgraph-bugs-found-in-port.md` and the port's expected result: a string value holds valid UTF-8, so
/// these inputs are not expressible (the user's Q4, entries 17, 18 and 31); converting their bytes to a string is
/// refused with `Value is not valid UTF-8 for type string`.
const NOT_EXPRESSIBLE: &[(&str, &str, &str)] = &[
    ("en", "17, 18, 31", "not expressible"),
    ("hu", "17, 18, 31", "not expressible"),
    ("da", "17, 18, 31", "not expressible"),
    ("cs", "17, 18, 31", "not expressible"),
    ("ja", "17, 18, 31", "not expressible"),
    ("fr-CA", "17, 18, 31", "not expressible"),
    ("en-u-kn-true", "17, 18, 31", "not expressible"),
    ("en-u-ks-identic", "17, 18, 31", "not expressible"),
    ("en-u-ka-shifted", "17, 18, 31", "not expressible"),
    ("en-u-ks-level1", "17, 18, 31", "not expressible"),
    ("xx-invalid-tag-!", "17, 18, 31", "not expressible"),
];

#[test]
fn test_golden_language_sort_orders() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_sort_lang.json");
    let golden = ultragraphdb_test_bdd::load_golden::<LangSortCase>(&path);
    assert!(golden.cases().len() >= 33);
    let (mut checked, mut not_expressible) = (0, 0);
    for c in golden.cases() {
        let input: Result<Vec<String>, _> = c
            .in_hex
            .iter()
            .map(|s| String::from_utf8(unhex(s)))
            .collect();
        let Ok(input) = input else {
            assert!(
                NOT_EXPRESSIBLE.iter().any(|&(lang, _, _)| lang == c.lang),
                "{}: invalid UTF-8 in a case not listed",
                c.lang
            );
            // Go sorts these bytes as strings; the port refuses them as a string value.
            for b in c.in_hex.iter().map(|s| unhex(s)) {
                if std::str::from_utf8(&b).is_err() {
                    let err = convert(&Val::bytes(TypeId::STRING, b), TypeId::STRING).unwrap_err();
                    assert_eq!(err.to_string(), "Value is not valid UTF-8 for type string");
                }
            }
            not_expressible += 1;
            continue;
        };
        let mut rows: Vec<Vec<Val>> = input
            .into_iter()
            .map(|s| vec![Val::new(TypeId::STRING, Value::Str(s))])
            .collect();
        let mut uids = get_uid_list(rows.len());
        sort(&mut rows, &mut uids, &[false], &c.lang, &TextCollator).unwrap();
        let out: Vec<Vec<u8>> = rows
            .iter()
            .map(|r| match r[0].value() {
                Some(Value::Str(s)) => s.as_bytes().to_vec(),
                other => panic!("{other:?}"),
            })
            .collect();
        let want: Vec<Vec<u8>> = c.out_hex.iter().map(|s| unhex(s)).collect();
        assert_eq!(out, want, "{}", c.lang);
        assert_eq!(uids, c.uids, "{}", c.lang);
        checked += 1;
    }
    assert_eq!(
        not_expressible,
        NOT_EXPRESSIBLE.len(),
        "every listed case is in the golden file"
    );
    assert!(checked >= 22, "{checked} language sorts checked");
}

#[test]
fn test_sort_of_mismatched_shapes_is_refused() {
    // Dgraph indexes the uids, directions and the second row by the first's shape and panics past them.
    let int = |i: i64| Val::new(TypeId::INT, Value::Int(i));
    let mut values = vec![vec![int(2), int(1)], vec![int(1)]];
    let err = sort(&mut values, &mut [1, 2], &[false, false], "", &TextCollator).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Sort row 1 holds 1 values where the first holds 2"
    );
    let mut values = vec![vec![int(2)], vec![int(1)]];
    let err = sort(&mut values, &mut [1], &[false], "", &TextCollator).unwrap_err();
    assert_eq!(err.to_string(), "Sort has 2 rows of values but 1 uids");
    let err = sort(&mut values, &mut [1, 2], &[], "", &TextCollator).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Sort has 0 directions for rows of 1 values"
    );
    let err = sort_top_n(&mut values, &mut [1, 2, 3], &[false], "", 1, &TextCollator).unwrap_err();
    assert_eq!(err.to_string(), "Sort has 2 rows of values but 3 uids");
    let mut facets = vec![pb::Facets::default()];
    let err = sort_with_facet(
        &mut values,
        &mut [1, 2],
        Some(&mut facets),
        &[false],
        "",
        &TextCollator,
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Sort has 2 rows of values but 1 facets");
    // Empty rows are allowed beside full ones, as Dgraph's Less returns false for them: an empty row is never
    // less nor greater, so Go's insertion sort of three rows leaves them where they are.
    let mut values = vec![vec![int(2)], vec![], vec![int(1)]];
    let mut uids = [1, 2, 3];
    sort(&mut values, &mut uids, &[false], "", &TextCollator).unwrap();
    assert_eq!(uids, [1, 2, 3]);
}

#[test]
fn test_is_sortable() {
    // Dgraph's IsSortable: datetime, int, float, string, default and bigfloat.
    for (tid, want) in [
        (TypeId::DEFAULT, true),
        (TypeId::BINARY, false),
        (TypeId::INT, true),
        (TypeId::FLOAT, true),
        (TypeId::BOOL, false),
        (TypeId::DATETIME, true),
        (TypeId::GEO, false),
        (TypeId::UID, false),
        (TypeId::PASSWORD, false),
        (TypeId::STRING, true),
        (TypeId::OBJECT, false),
        (TypeId::BIGFLOAT, true),
        (TypeId::VFLOAT, false),
    ] {
        assert_eq!(is_sortable(tid), want, "{}", tid.name());
    }
}

fn facet(key: &str) -> pb::Facets {
    pb::Facets {
        facets: vec![api::Facet {
            key: key.into(),
            ..Default::default()
        }],
    }
}

#[test]
fn test_sort_with_facet_moves_facets_with_their_rows() {
    let int = |i: i64| vec![Val::new(TypeId::INT, Value::Int(i))];
    let mut values = vec![int(3), int(1), int(2)];
    let mut uids = [10, 20, 30];
    let mut facets = vec![facet("a"), facet("b"), facet("c")];
    sort_with_facet(
        &mut values,
        &mut uids,
        Some(&mut facets),
        &[false],
        "",
        &TextCollator,
    )
    .unwrap();
    assert_eq!(uids, [20, 30, 10]);
    assert_eq!(facets, [facet("b"), facet("c"), facet("a")]);
}

#[test]
fn test_sort_nil_values_last_and_ties_by_next_value() {
    let int = |i: i64| Val::new(TypeId::INT, Value::Int(i));
    let nil = Val::nil(TypeId::INT);
    // Dgraph sorts a nil value last whichever the direction.
    for (desc, want) in [(false, [300, 200, 100]), (true, [200, 300, 100])] {
        let mut values = vec![vec![nil.clone()], vec![int(2)], vec![int(1)]];
        let mut uids = [100, 200, 300];
        sort(&mut values, &mut uids, &[desc], "", &TextCollator).unwrap();
        assert_eq!(uids, want, "desc {desc}");
    }
    // Equal first values are ordered by the second, in its own direction.
    let st = |v: &str| Val::new(TypeId::STRING, Value::str(v));
    for (desc, want) in [
        ([false, false], [300, 200, 100]),
        ([false, true], [300, 100, 200]),
    ] {
        let mut values = vec![
            vec![int(1), st("b")],
            vec![int(1), st("a")],
            vec![int(0), st("z")],
        ];
        let mut uids = [100, 200, 300];
        sort(&mut values, &mut uids, &desc, "", &TextCollator).unwrap();
        assert_eq!(uids, want, "desc {desc:?}");
    }
}
