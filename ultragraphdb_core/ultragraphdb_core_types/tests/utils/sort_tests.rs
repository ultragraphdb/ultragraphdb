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
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn to_string(values: &[Vec<Val>]) -> TestResult<Vec<String>> {
    values
        .iter()
        .map(|v| {
            match marshal(&v[0], TypeId::STRING)?.into_value() {
                Some(Value::Str(s)) => Ok(s),
                other => Err(format!("{other:?}")),
            }
            .context("marshalling to string gives a string")
        })
        .collect()
}

fn get_input(tid: TypeId, input: &[&str]) -> TestResult<Vec<Vec<Val>>> {
    input
        .iter()
        .map(|s| {
            Ok(vec![convert(
                &Val::bytes(TypeId::STRING, s.as_bytes()),
                tid,
            )?])
        })
        .collect()
}

fn get_uid_list(n: usize) -> Vec<u64> {
    (1..=n as u64).map(|i| i * 100).collect()
}

#[test]
fn test_quick_select() -> TestResult {
    let (n, k) = (10000, 10);
    let mut seed: u64 = 7;
    let mut list: Vec<Vec<Val>> = (0..n)
        .map(|_| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let s = ((seed >> 33) % 100000).to_string();
            Ok(vec![convert(
                &Val::bytes(TypeId::STRING, s.into_bytes()),
                TypeId::INT,
            )?])
        })
        .collect::<TestResult<_>>()?;
    let mut ul = get_uid_list(n);
    sort_top_n(&mut list, &mut ul, &[false], "", k, &TextCollator)?;
    let int = |v: &Val| {
        match v.value() {
            Some(Value::Int(i)) => Ok(*i),
            other => Err(format!("{other:?}")),
        }
        .context("an int value")
    };
    for i in 0..k {
        for j in k..n {
            assert!(int(&list[i][0])? <= int(&list[j][0])?);
        }
    }
    // SortTopN also sorts the first k, and the uids move with their values.
    for w in list[..k].windows(2) {
        assert!(int(&w[0][0])? <= int(&w[1][0])?);
    }
    let mut replay_seed: u64 = 7;
    let original: Vec<i64> = (0..n)
        .map(|_| {
            replay_seed = replay_seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            i64::try_from((replay_seed >> 33) % 100000).context("below 100000")
        })
        .collect::<TestResult<_>>()?;
    for (row, uid) in list.iter().zip(&ul) {
        assert_eq!(
            int(&row[0])?,
            original[usize::try_from(*uid / 100 - 1)?],
            "uid {uid}"
        );
    }
    Ok(())
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
fn int_values(rows: &[Vec<Val>]) -> TestResult<Vec<Option<i64>>> {
    rows.iter()
        .map(|r| {
            match r[0].value() {
                Some(Value::Int(i)) => Ok(Some(*i)),
                None => Ok(None),
                other => Err(format!("{other:?}")),
            }
            .context("an int or nil value")
        })
        .collect()
}

/// How two optional ints order in a sort: a nil after every value whatever the direction.
fn int_order(a: Option<i64>, b: Option<i64>, desc: bool) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(x), Some(y)) if desc => y.cmp(&x),
        (Some(x), Some(y)) => x.cmp(&y),
    }
}

/// Runs `sort_top_n` over `input` for each `n`. The first `n` rows are the `n` smallest in order, every row keeps
/// its uid, and a count that takes in every row gives the stable order: equal rows as they came in.
fn assert_top_n(input: &[Option<i64>], desc: bool, ns: &[usize]) -> TestResult {
    let uid_list = get_uid_list(input.len());
    let mut stable: Vec<(Option<i64>, u64)> = input.iter().copied().zip(uid_list).collect();
    let came_in = stable.clone();
    stable.sort_by(|a, b| int_order(a.0, b.0, desc));
    for &n in ns {
        let (mut rows, mut ul) = int_rows(input);
        sort_top_n(&mut rows, &mut ul, &[desc], "", n, &TextCollator)?;
        let got: Vec<(Option<i64>, u64)> = int_values(&rows)?.into_iter().zip(ul).collect();
        let front = n.min(input.len());
        let values = |pairs: &[(Option<i64>, u64)]| -> Vec<Option<i64>> {
            pairs.iter().take(front).map(|pair| pair.0).collect()
        };
        assert_eq!(values(&got), values(&stable), "desc {desc}, n {n}");
        let mut by_uid = got.clone();
        by_uid.sort_by_key(|pair| pair.1);
        assert_eq!(by_uid, came_in, "each row with its uid, desc {desc}, n {n}");
        if n >= input.len() {
            assert_eq!(got, stable, "desc {desc}, n {n}");
        }
    }
    Ok(())
}

#[test]
fn test_sort_top_n_with_nil_values_and_ties() -> TestResult {
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
    let ns = [0, 1, 2, 3, 5, 6, 7, 8, 9, 100, usize::MAX];
    assert_top_n(&input, false, &ns)?;
    assert_top_n(&input, true, &ns)
}

#[test]
fn test_sort_top_n_counts_past_the_rows_sort_every_row() -> TestResult {
    // Twelve values and a nil: a count of 13 or more sorts the nil row too.
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
    assert_top_n(&input, false, &[0, 1, 4, 5, 12, 13, 14, usize::MAX])
}

#[test]
fn test_sort_top_n_leaves_the_rest_with_their_uids() -> TestResult {
    let input: Vec<Option<i64>> = [
        14, 2, 27, 2, 9, 30, 5, 18, 2, 21, 11, 0, 25, 7, 16, 9, 3, 29, 12, 1, 23, 9, 6, 19, 8, 28,
        4, 15, 10, 22,
    ]
    .into_iter()
    .map(Some)
    .collect();
    let ns = [0, 1, 3, 4, 10, 15, 29, 30, 31];
    assert_top_n(&input, false, &ns)?;
    assert_top_n(&input, true, &ns)
}

#[test]
fn test_sort_top_n_of_no_rows() -> TestResult {
    for n in [0, 1, 5, usize::MAX] {
        let (mut rows, mut ul) = int_rows(&[]);
        sort_top_n(&mut rows, &mut ul, &[false], "", n, &TextCollator)?;
        assert!(rows.is_empty(), "n {n}");
        assert!(ul.is_empty(), "n {n}");
    }
    Ok(())
}

#[test]
fn test_sort_strings() -> TestResult {
    let mut list = get_input(TypeId::STRING, &["bb", "aaa", "aa", "bab"])?;
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator)?;
    assert_eq!(ul, [300, 200, 400, 100]);
    assert_eq!(to_string(&list)?, ["aa", "aaa", "bab", "bb"]);
    Ok(())
}

#[test]
fn test_sort_language() -> TestResult {
    let mut list = get_input(TypeId::STRING, &["öffnen", "zumachen"])?;
    let mut ul = get_uid_list(2);
    sort(&mut list, &mut ul, &[false], "de", &TextCollator)?;
    assert_eq!(ul, [100, 200]);
    assert_eq!(to_string(&list)?, ["öffnen", "zumachen"]);

    let mut swedish = get_input(TypeId::STRING, &["öppna", "zon"])?;
    sort(&mut swedish, &mut ul, &[false], "sv", &TextCollator)?;
    assert_eq!(ul, [200, 100]);
    assert_eq!(to_string(&swedish)?, ["zon", "öppna"]);
    Ok(())
}

#[test]
fn test_sort_ints() -> TestResult {
    let mut list = get_input(TypeId::INT, &["22", "111", "11", "212"])?;
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator)?;
    assert_eq!(ul, [300, 100, 200, 400]);
    assert_eq!(to_string(&list)?, ["11", "22", "111", "212"]);
    Ok(())
}

#[test]
fn test_sort_floats() -> TestResult {
    let mut list = get_input(TypeId::FLOAT, &["22.2", "11.2", "11.5", "2.12"])?;
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator)?;
    assert_eq!(ul, [400, 200, 300, 100]);
    assert_eq!(to_string(&list)?, ["2.12", "11.2", "11.5", "22.2"]);
    Ok(())
}

#[test]
fn test_sort_floats_desc() -> TestResult {
    let mut list = get_input(TypeId::FLOAT, &["22.2", "11.2", "11.5", "2.12"])?;
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[true], "", &TextCollator)?;
    assert_eq!(ul, [100, 300, 200, 400]);
    assert_eq!(to_string(&list)?, ["22.2", "11.5", "11.2", "2.12"]);
    Ok(())
}

#[test]
fn test_sort_date_times() -> TestResult {
    let input = [
        "2016-01-02T15:04:05",
        "2006-01-02T15:04:05",
        "2006-01-02T15:04:06",
        "2006-01-02T15:04:01",
    ];
    let mut list = get_input(TypeId::DATETIME, &input)?;
    let mut ul = get_uid_list(4);
    sort(&mut list, &mut ul, &[false], "", &TextCollator)?;
    assert_eq!(ul, [400, 200, 300, 100]);
    assert_eq!(
        to_string(&list)?,
        [
            "2006-01-02T15:04:01Z",
            "2006-01-02T15:04:05Z",
            "2006-01-02T15:04:06Z",
            "2016-01-02T15:04:05Z"
        ]
    );
    Ok(())
}

#[test]
fn test_sort_int_and_float() -> TestResult {
    let mut list = vec![
        vec![Val::new(TypeId::INT, Value::Int(55))],
        vec![Val::new(TypeId::FLOAT, Value::Float(21.5))],
        vec![Val::new(TypeId::INT, Value::Int(100))],
    ];
    let mut ul = get_uid_list(3);
    sort(&mut list, &mut ul, &[false], "", &TextCollator)?;
    assert_eq!(ul, [200, 100, 300]);
    assert_eq!(to_string(&list)?, ["21.5", "55", "100"]);
    Ok(())
}

#[test]
fn test_equal() -> TestResult {
    let int = |i| Val::new(TypeId::INT, Value::Int(i));
    assert!(equal(&int(3), &int(3))?);
    assert!(!equal(&int(3), &int(4))?);
    assert_eq!(
        equal(&int(3), &Val::new(TypeId::FLOAT, Value::Float(3.0)))
            .err()
            .context("expected an error")?
            .to_string(),
        "Arguments of different type can not be compared."
    );
    assert!(!equal(&int(3), &Val::nil(TypeId::INT))?);
    assert!(!equal(&Val::nil(TypeId::INT), &int(3))?);
    assert!(!equal(&Val::nil(TypeId::INT), &Val::nil(TypeId::INT))?);
    let int_with_float = Val::new(TypeId::INT, Value::Float(3.0));
    assert!(!equal(
        &int_with_float,
        &Val::new(TypeId::INT, Value::Float(3.0))
    )?);
    assert!(!equal(
        &Val::new(TypeId::FLOAT, Value::Float(3.0)),
        &Val::new(TypeId::FLOAT, Value::Int(3))
    )?);
    Ok(())
}

fn find_index(uids: &[u64], uid: u64) -> TestResult<usize> {
    uids.iter()
        .position(|u| *u == uid)
        .context("Could not find index")
}

#[test]
fn test_sort_mismatched_types() -> TestResult {
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
    sort(&mut list, &mut ul, &[false], "", &TextCollator)?;
    assert!(find_index(&ul, 500)? < find_index(&ul, 600)?);
    assert!(find_index(&ul, 600)? < find_index(&ul, 100)?);
    assert!(find_index(&ul, 400)? < find_index(&ul, 700)?);
    assert!(find_index(&ul, 700)? < find_index(&ul, 200)?);
    Ok(())
}

#[test]
fn test_less_errors() -> TestResult {
    let int = Val::new(TypeId::INT, Value::Int(1));
    assert_eq!(
        less(&int, &Val::new(TypeId::FLOAT, Value::Float(1.0)))
            .err()
            .context("expected an error")?
            .to_string(),
        "Arguments of different type can not be compared."
    );
    let b = Val::new(TypeId::BOOL, Value::Bool(true));
    assert_eq!(
        less(&b, &b).err().context("expected an error")?.to_string(),
        "Compare not supported for type: 4"
    );
    let g = Val::new(TypeId::GEO, Value::Bytes(Vec::new()));
    assert_eq!(
        equal(&g, &g)
            .err()
            .context("expected an error")?
            .to_string(),
        "Equal not supported for type: 6"
    );
    Ok(())
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

fn unhex(s: &str) -> TestResult<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).context("a hex byte"))
        .collect()
}

/// A golden case's rows as values.
fn golden_rows(c: &SortCase) -> TestResult<Vec<Vec<Val>>> {
    c.rows
        .iter()
        .flatten()
        .map(|r| {
            r.iter()
                .map(|v| -> TestResult<Val> {
                    let tid = TypeId::new(v.tid);
                    Ok(if v.nil {
                        Val::nil(tid)
                    } else if tid == TypeId::UID {
                        let b = unhex(&v.bin)?;
                        Val::new(
                            tid,
                            Value::Uid(u64::from_le_bytes(
                                *b.first_chunk().context("the 8 bytes of a uid")?,
                            )),
                        )
                    } else {
                        convert(&Val::bytes(TypeId::BINARY, unhex(&v.bin)?), tid)?
                    })
                })
                .collect()
        })
        .collect()
}

/// A row's keys as a sort reads them: a nil key is nil whatever its type, and an int and a float of one number
/// are one key.
fn row_keys(row: &[Val]) -> String {
    let keys: Vec<Option<String>> = row
        .iter()
        .map(|v| {
            v.value().map(|x| match x {
                Value::Int(i) => format!("{}", *i as f64),
                Value::Float(f) => format!("{f}"),
                Value::Bytes(_)
                | Value::Str(_)
                | Value::Bool(_)
                | Value::DateTime(_)
                | Value::Geo(_)
                | Value::Uid(_)
                | Value::Decimal(_)
                | Value::VFloat(_) => format!("{:?}", (v.tid(), x)),
            })
        })
        .collect();
    format!("{keys:?}")
}

/// Go's sort of each golden case and this crate's agree on the rows sorted that hold a first key, compared by
/// their keys in the order of Go's uids. Go orders equal rows and the rows past a count as its own sort leaves
/// them, and two rows without a first key by the direction alone; rows of several keys whose first mixes types
/// are left to this crate's own tests.
#[test]
fn test_golden_sort_orders() -> TestResult {
    let path = ultragraphdb_tool_bdd::testdata_dir()?.join("golden/types_sort.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<SortCase>(&path)?;
    for c in golden.cases() {
        let mut rows = golden_rows(c)?;
        let mut uids = get_uid_list(rows.len());
        let res = if c.top_n > 0 {
            sort_top_n(&mut rows, &mut uids, &c.desc, "", c.top_n, &TextCollator)
        } else {
            sort(&mut rows, &mut uids, &c.desc, "", &TextCollator)
        };
        if let Err(e) = res {
            assert_eq!(e.to_string(), c.err, "{}", c.name);
            continue;
        }
        // Go takes an int and a float of one number for different keys and never reads the key after them.
        let first_types: std::collections::HashSet<TypeId> = rows
            .iter()
            .filter_map(|row| row.first().filter(|v| v.value().is_some()).map(Val::tid))
            .collect();
        if c.desc.len() > 1 && first_types.len() > 1 {
            continue;
        }
        let keyed = rows
            .iter()
            .filter(|row| row.first().is_some_and(|v| v.value().is_some()))
            .count();
        let sorted = if c.top_n > 0 { c.top_n } else { rows.len() }.min(keyed);
        let by_uid: std::collections::HashMap<u64, &Vec<Val>> =
            uids.iter().copied().zip(&rows).collect();
        let want: Vec<Option<String>> = c
            .uids
            .iter()
            .flatten()
            .take(sorted)
            .map(|uid| by_uid.get(uid).map(|row| row_keys(row)))
            .collect();
        let got: Vec<Option<String>> = rows
            .iter()
            .take(sorted)
            .map(|row| Some(row_keys(row)))
            .collect();
        assert_eq!(got, want, "{}", c.name);
    }
    Ok(())
}

/// A sort by a language orders strings as the language collates them, and by their bytes with no language: Swedish
/// sorts `ä` after `z`, German before `b`, and bytes order every upper-case letter first.
#[test]
fn test_a_sort_by_a_language_orders_as_the_language_collates() -> TestResult {
    let words = ["zebra", "ä", "b", "Apfel", "apfel"];
    for (lang, want) in [
        ("sv", ["apfel", "Apfel", "b", "zebra", "ä"]),
        ("de", ["ä", "apfel", "Apfel", "b", "zebra"]),
        ("", ["Apfel", "apfel", "b", "zebra", "ä"]),
        ("xx-invalid-tag-!", ["Apfel", "apfel", "b", "zebra", "ä"]),
    ] {
        let mut rows: Vec<Vec<Val>> = words
            .iter()
            .map(|s| vec![Val::new(TypeId::STRING, Value::Str((*s).to_string()))])
            .collect();
        let mut uids = get_uid_list(rows.len());
        sort(&mut rows, &mut uids, &[false], lang, &TextCollator)?;
        let out: Vec<Option<&str>> = rows
            .iter()
            .map(|r| match r.first().and_then(Val::value) {
                Some(Value::Str(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(out, want.map(Some), "{lang:?}");
    }
    Ok(())
}

#[test]
fn test_sort_of_mismatched_shapes_is_refused() -> TestResult {
    // Dgraph indexes the uids, directions and the second row by the first's shape and panics past them.
    let int = |i: i64| Val::new(TypeId::INT, Value::Int(i));
    let mut values = vec![vec![int(2), int(1)], vec![int(1)]];
    let err = sort(&mut values, &mut [1, 2], &[false, false], "", &TextCollator)
        .err()
        .context("expected an error")?;
    assert_eq!(
        err.to_string(),
        "Sort row 1 holds 1 values where the first holds 2"
    );
    let mut two_values = vec![vec![int(2)], vec![int(1)]];
    let uids_err = sort(&mut two_values, &mut [1], &[false], "", &TextCollator)
        .err()
        .context("expected an error")?;
    assert_eq!(uids_err.to_string(), "Sort has 2 rows of values but 1 uids");
    let desc_err = sort(&mut two_values, &mut [1, 2], &[], "", &TextCollator)
        .err()
        .context("expected an error")?;
    assert_eq!(
        desc_err.to_string(),
        "Sort has 0 directions for rows of 1 values"
    );
    let top_n_err = sort_top_n(
        &mut two_values,
        &mut [1, 2, 3],
        &[false],
        "",
        1,
        &TextCollator,
    )
    .err()
    .context("expected an error")?;
    assert_eq!(
        top_n_err.to_string(),
        "Sort has 2 rows of values but 3 uids"
    );
    // Empty rows are allowed beside full ones: an empty row holds no key, and goes after every row that does.
    let mut with_empty = vec![vec![int(2)], vec![], vec![int(1)]];
    let mut uids = [1, 2, 3];
    sort(&mut with_empty, &mut uids, &[false], "", &TextCollator)?;
    assert_eq!(uids, [3, 1, 2]);
    Ok(())
}

#[test]
fn test_is_sortable() {
    // Dgraph's IsSortable: datetime, int, float, string, default and decimal.
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
        (TypeId::DECIMAL, true),
        (TypeId::VFLOAT, false),
    ] {
        assert_eq!(is_sortable(tid), want, "{}", tid.name());
    }
}

#[test]
fn test_sort_nil_values_last_and_ties_by_next_value() -> TestResult {
    let int = |i: i64| Val::new(TypeId::INT, Value::Int(i));
    let nil = Val::nil(TypeId::INT);
    // Dgraph sorts a nil value last whichever the direction.
    for (desc, want) in [(false, [300, 200, 100]), (true, [200, 300, 100])] {
        let mut values = vec![vec![nil.clone()], vec![int(2)], vec![int(1)]];
        let mut uids = [100, 200, 300];
        sort(&mut values, &mut uids, &[desc], "", &TextCollator)?;
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
        sort(&mut values, &mut uids, &desc, "", &TextCollator)?;
        assert_eq!(uids, want, "desc {desc:?}");
    }
    Ok(())
}

/// The uids a sort of one-key rows leaves, the rows numbered from 1.
fn sorted_uids(values: Vec<Val>, desc: bool) -> TestResult<Vec<u64>> {
    let mut rows: Vec<Vec<Val>> = values.into_iter().map(|v| vec![v]).collect();
    let mut uids: Vec<u64> = (1..).take(rows.len()).collect();
    sort(&mut rows, &mut uids, &[desc], "", &TextCollator)?;
    Ok(uids)
}

/// An int and a float order by their exact numbers, past the ints a float holds exactly, and a NaN goes past
/// the infinity of its sign, so every float has a place.
#[test]
fn test_ints_and_floats_order_by_their_exact_numbers() -> TestResult {
    let int = |i: i64| Val::new(TypeId::INT, Value::Int(i));
    let float = |f: f64| Val::new(TypeId::FLOAT, Value::Float(f));
    // Two to the 53rd as a float lies between the ints beside it, which round to it.
    let big = 9_007_199_254_740_992_i64;
    assert_eq!(
        sorted_uids(vec![int(big + 1), float(big as f64), int(big - 1)], false)?,
        [3, 2, 1]
    );
    // Two to the 63rd is past every int.
    assert_eq!(
        sorted_uids(
            vec![float(9_223_372_036_854_775_808.0), int(i64::MAX)],
            false
        )?,
        [2, 1]
    );
    assert_eq!(
        sorted_uids(
            vec![
                float(f64::NAN),
                int(0),
                float(f64::INFINITY),
                float(-f64::NAN),
                float(f64::NEG_INFINITY),
            ],
            false
        )?,
        [4, 5, 2, 3, 1]
    );
    Ok(())
}

/// Rows that compare equal keep the order they came in, in either direction, and rows without a first key go
/// last, ordered by the next key.
#[test]
fn test_equal_rows_keep_their_order_and_keyless_rows_go_last() -> TestResult {
    let int = |i: i64| Val::new(TypeId::INT, Value::Int(i));
    for desc in [false, true] {
        assert_eq!(sorted_uids(vec![int(7), int(7), int(7)], desc)?, [1, 2, 3]);
    }
    let nil = || Val::nil(TypeId::INT);
    let mut rows = vec![
        vec![nil(), int(2)],
        vec![int(5), int(1)],
        vec![nil(), int(1)],
        vec![nil(), nil()],
        vec![int(5), nil()],
    ];
    let mut uids = [1, 2, 3, 4, 5];
    sort(&mut rows, &mut uids, &[true, false], "", &TextCollator)?;
    assert_eq!(uids, [2, 5, 3, 1, 4]);
    Ok(())
}
