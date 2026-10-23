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
    TextCollator, TypeId, Val, Value, convert, equal, less, marshal, sort, sort_top_n,
};

fn to_string(values: &[Vec<Val>]) -> Vec<String> {
    values
        .iter()
        .map(
            |v| match marshal(&v[0], TypeId::STRING).unwrap().into_value() {
                Some(Value::Str(s)) => String::from_utf8(s).unwrap(),
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
    assert!(equal(&int(3), &Val::new(TypeId::FLOAT, Value::Float(3.0))).is_err());
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

#[test]
fn test_golden_language_sort_orders() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_sort_lang.json");
    let golden = ultragraphdb_test_bdd::load_golden::<LangSortCase>(&path);
    assert!(golden.cases().len() >= 33);
    let mut invalid = 0;
    for c in golden.cases() {
        let input: Vec<Vec<u8>> = c.in_hex.iter().map(|s| unhex(s)).collect();
        invalid += input
            .iter()
            .filter(|s| std::str::from_utf8(s).is_err())
            .count();
        let mut rows: Vec<Vec<Val>> = input
            .into_iter()
            .map(|s| vec![Val::new(TypeId::STRING, Value::Str(s))])
            .collect();
        let mut uids = get_uid_list(rows.len());
        sort(&mut rows, &mut uids, &[false], &c.lang, &TextCollator).unwrap();
        let out: Vec<Vec<u8>> = rows
            .iter()
            .map(|r| match r[0].value() {
                Some(Value::Str(s)) => s.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        let want: Vec<Vec<u8>> = c.out_hex.iter().map(|s| unhex(s)).collect();
        assert_eq!(out, want, "{}", c.lang);
        assert_eq!(uids, c.uids, "{}", c.lang);
    }
    assert_eq!(invalid, 11 * 27, "strings holding invalid UTF-8");
}
