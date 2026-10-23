/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{get_index_factory, get_tokenizer};
use ultragraphdb_core_types::{TypeId, Value};
use ultragraphdb_traits_index::{Metric, OptionValue, Options};

#[test]
fn test_hnsw_factory() {
    let f = get_index_factory("hnsw").unwrap();
    assert_eq!(f.name(), "hnsw");
    assert_eq!(f.float_bits(), 32);
    assert!(get_index_factory("HNSW").is_none());
    let mut names = f.allowed_options().get_option_list();
    names.sort();
    assert_eq!(
        names,
        vec![
            "efConstruction",
            "efSearch",
            "exponent",
            "maxLevels",
            "metric"
        ]
    );
    assert_eq!(
        f.allowed_options()
            .get_parsed_option("metric", "manhattan")
            .unwrap_err()
            .to_string(),
        "Can't create a vector index for manhattan"
    );
    assert_eq!(f.get_options(&Options::new()), "");
}

#[test]
fn test_hnsw_get_options_of_wrong_types() {
    let f = get_index_factory("hnsw").unwrap();
    let mut o = Options::new();
    o.set_opt("exponent", OptionValue::Str("x".into()))
        .set_opt("metric", OptionValue::Int(1));
    // Go prints the default 3 for an int option of another type, and an empty metric name.
    assert_eq!(f.get_options(&o), "(\"exponent\":\"3\",\"metric\":\"\")");
    o.set_opt("metric", OptionValue::Metric(Metric::DotProduct));
    assert_eq!(
        f.get_options(&o),
        "(\"exponent\":\"3\",\"metric\":\"dotproduct\")"
    );
}

#[test]
fn test_hnsw_as_tokenizer() {
    let t = get_tokenizer("hnsw").unwrap();
    assert_eq!(t.identifier(), 0x0E);
    assert!(t.is_valid_for(TypeId::VFLOAT));
    assert!(t.index_factory().is_some());
    assert_eq!(
        t.tokens(&Value::VFloat(vec![1.0])).unwrap(),
        vec![b"float".to_vec()]
    );
}

#[test]
fn test_metric_names() {
    for m in [Metric::Euclidean, Metric::Cosine, Metric::DotProduct] {
        assert_eq!(Metric::from_name(m.name()), Some(m));
    }
    assert_eq!(Metric::from_name("manhattan"), None);
}
