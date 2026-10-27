/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestRaceCreateOrReplace, TestCreatepersistentHNSW).
 * Changed for ultragraphdb: translated to Rust; the factory's map of indexes is not ported, so the race is between
 * builds of indexes; the float width is the vectors', where Go's test builds float64 indexes of 32-bit floats;
 * added: the parameters and refusals of the options, against Go's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use serde::Deserialize;
use ultragraphdb_error_options::OptionsErrorEnum;
use ultragraphdb_store_hnsw::{HnswErrorEnum, PersistentHnsw, SimilarityType};
use ultragraphdb_tool_bdd::{load_golden, testdata_dir};
use ultragraphdb_traits_index::{OptionValue, Options};

use crate::utils::helpers::{opts, vkeys};

/// Dgraph's `optionsFromCreateTestCase`.
fn options(max_levels: i64, ef_search: i64, ef_construction: i64, index_type: &str) -> Options {
    let mut o = Options::new();
    o.set_opt("maxLevels", OptionValue::Int(max_levels))
        .set_opt("efSearch", OptionValue::Int(ef_search))
        .set_opt("efConstruction", OptionValue::Int(ef_construction))
        .set_opt(
            "metric",
            OptionValue::Metric(SimilarityType::from_name(index_type).metric()),
        );
    o
}

#[allow(
    clippy::disallowed_methods,
    reason = "the test races builds of an index on threads of its own, as Go's goroutines do"
)]
#[test]
fn test_race_create_or_replace() {
    let o = options(1, 1, 1, "b");
    std::thread::scope(|s| {
        for _ in 0..5 {
            s.spawn(|| {
                for i in 0..10 {
                    let index = PersistentHnsw::<f64>::new(vkeys("a"), &o, 64)
                        .unwrap_or_else(|e| panic!("case {i}: {e}"));
                    assert_eq!(
                        (
                            index.max_levels(),
                            index.ef_search(),
                            index.ef_construction()
                        ),
                        (1, 1, 1),
                        "case {i}"
                    );
                }
            });
        }
    });
}

#[test]
fn test_create_persistent_hnsw() {
    for (index_type, expected) in [
        ("b", "euclidean"),
        ("euclidean", "euclidean"),
        ("cosine", "cosine"),
        ("dotproduct", "dotproduct"),
    ] {
        let index =
            PersistentHnsw::<f64>::new(vkeys("a"), &options(1, 1, 1, index_type), 64).unwrap();
        assert_eq!(index.sim_type().index_type(), expected);
        assert_eq!(
            (
                index.max_levels(),
                index.ef_search(),
                index.ef_construction()
            ),
            (1, 1, 1)
        );
        let keys = index.keys();
        assert_eq!(
            (
                keys.name().to_string(),
                keys.neighbours_label(),
                keys.entry_label(),
                keys.dead_label()
            ),
            (
                "a".to_string(),
                "a__vector_".to_string(),
                "a__vector_entry".to_string(),
                "a__vector_dead".to_string()
            )
        );
    }
}

/// Deviation: Go's test builds float64 indexes with 32 float bits, which reinterpret a float64 vector's memory as
/// float32s at the first distance; the port refuses a width other than the vectors' type's.
#[test]
fn test_a_float_width_other_than_the_vectors_is_refused() {
    let o = options(1, 1, 1, "b");
    for bits in [32, 16, 0] {
        let e = PersistentHnsw::<f64>::new(vkeys("a"), &o, bits).unwrap_err();
        assert_eq!(e.0, HnswErrorEnum::FloatBits { bits, expected: 64 });
    }
    let f32_index = PersistentHnsw::<f32>::new(vkeys("a"), &o, 32).unwrap();
    assert_eq!(f32_index.sim_type().index_type(), "euclidean");
    let e = PersistentHnsw::<f32>::new(vkeys("a"), &o, 64).unwrap_err();
    assert_eq!(
        e.0,
        HnswErrorEnum::FloatBits {
            bits: 64,
            expected: 32
        }
    );
}

/// Deviation: Go accepts a level count or breadth below 1 and panics at the first search or insert that indexes the
/// level -1 or truncates a layer result to nothing; the port refuses it when the index is built.
#[test]
fn test_an_option_below_one_is_refused() {
    for (name, value, o) in [
        ("maxLevels", 0, opts(&[("maxLevels", "0")])),
        ("efSearch", -3, opts(&[("efSearch", "-3")])),
        ("efConstruction", 0, opts(&[("efConstruction", "0")])),
        ("maxLevels", 0, opts(&[("exponent", "0")])),
        ("maxLevels", -1, opts(&[("exponent", "-1")])),
        (
            "efConstruction",
            0,
            opts(&[("exponent", "2"), ("efConstruction", "0")]),
        ),
        ("efSearch", 0, opts(&[("exponent", "1"), ("efSearch", "0")])),
    ] {
        let e = PersistentHnsw::<f32>::new(vkeys("0-a"), &o, 32).unwrap_err();
        assert_eq!(e.0, HnswErrorEnum::InvalidParameter { name, value }, "{e}");
    }
    // 1 is the least accepted, directly and through an exponent of 1 (1, 50 and 30).
    let one = opts(&[
        ("maxLevels", "1"),
        ("efConstruction", "1"),
        ("efSearch", "1"),
    ]);
    let ix = PersistentHnsw::<f32>::new(vkeys("0-a"), &one, 32).unwrap();
    assert_eq!(
        (ix.max_levels(), ix.ef_construction(), ix.ef_search()),
        (1, 1, 1)
    );
    let ix = PersistentHnsw::<f32>::new(vkeys("0-a"), &opts(&[("exponent", "1")]), 32).unwrap();
    assert_eq!(
        (ix.max_levels(), ix.ef_construction(), ix.ef_search()),
        (1, 50, 30)
    );
}

#[derive(Debug, Deserialize)]
struct OptionsCase {
    name: String,
    options: BTreeMap<String, String>,
    max_levels: usize,
    ef_construction: usize,
    ef_search: usize,
    metric: String,
    error: String,
}

#[test]
fn test_options_set_gos_parameters() {
    let golden = load_golden::<OptionsCase>(&testdata_dir().join("golden/hnsw_options.json"));
    let mut compared = 0;
    for c in golden.cases() {
        let o = match c.name.as_str() {
            "wrong_type_maxLevels" | "wrong_type_efConstruction" | "wrong_type_efSearch" => {
                let which = c.name.trim_start_matches("wrong_type_");
                let mut o = Options::new();
                o.set_opt(which, OptionValue::Str("x".into()));
                o
            }
            "wrong_type_metric" => {
                let mut o = Options::new();
                o.set_opt("metric", OptionValue::Int(7));
                o
            }
            // The factory's map of names is not ported: every insert and search builds its own index.
            "name_taken" => continue,
            _ => {
                let pairs: Vec<(&str, &str)> = c
                    .options
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                opts(&pairs)
            }
        };
        match PersistentHnsw::<f32>::new(vkeys("0-opts"), &o, 32) {
            Ok(index) => {
                assert!(
                    c.error.is_empty(),
                    "{}: Go refused with {}",
                    c.name,
                    c.error
                );
                assert_eq!(
                    (
                        index.max_levels(),
                        index.ef_construction(),
                        index.ef_search(),
                        index.sim_type().index_type()
                    ),
                    (
                        c.max_levels,
                        c.ef_construction,
                        c.ef_search,
                        c.metric.as_str()
                    ),
                    "{}",
                    c.name
                );
            }
            Err(e) => assert_eq!(e.to_string(), c.error, "{}", c.name),
        }
        compared += 1;
    }
    assert_eq!(compared, 15);
}

#[test]
fn test_a_wrong_option_type_is_gos_error() {
    let mut o = Options::new();
    o.set_opt("maxLevels", OptionValue::Bool(true));
    let e = PersistentHnsw::<f32>::new(vkeys("0-a"), &o, 32).unwrap_err();
    assert!(
        matches!(e.0, HnswErrorEnum::Options(ref oe) if matches!(oe.0, OptionsErrorEnum::Type(_)))
    );
    // An exponent of another type is ignored and leaves the defaults, as Dgraph ignores its error.
    let mut o = Options::new();
    o.set_opt("exponent", OptionValue::Str("4".into()));
    let index = PersistentHnsw::<f32>::new(vkeys("0-a"), &o, 32).unwrap();
    assert_eq!(
        (
            index.max_levels(),
            index.ef_construction(),
            index.ef_search()
        ),
        (3, 150, 90)
    );
}
