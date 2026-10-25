/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use tracing::Level;
use ultragraphdb_core_tok::{get_index_factory, get_tokenizer};
use ultragraphdb_core_types::{TypeId, Value};
use ultragraphdb_store_sim::LogCapture;
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
    // GetPersistantOptions prints the set options in its own order, whatever order they were set in.
    let mut o = Options::new();
    o.set_opt("metric", OptionValue::Metric(Metric::Cosine))
        .set_opt("efSearch", OptionValue::Int(40))
        .set_opt("efConstruction", OptionValue::Int(-1))
        .set_opt("maxLevels", OptionValue::Int(0))
        .set_opt("exponent", OptionValue::Int(i64::MAX));
    assert_eq!(
        f.get_options(&o),
        "(\"exponent\":\"9223372036854775807\",\"maxLevels\":\"0\",\"efConstruction\":\"-1\",\
         \"efSearch\":\"40\",\"metric\":\"cosine\")"
    );
    let mut o = Options::new();
    o.set_opt("efSearch", OptionValue::Int(7));
    assert_eq!(f.get_options(&o), "(\"efSearch\":\"7\")");
}

#[test]
fn test_hnsw_get_options_of_wrong_types() {
    let f = get_index_factory("hnsw").unwrap();
    let mut o = Options::new();
    o.set_opt("exponent", OptionValue::Str("x".into()))
        .set_opt("metric", OptionValue::Int(1));
    // Go prints the default 3 for an int option of another type, and an empty metric name, logging the metric's
    // failed cast (persistent_hnsw.go's GetPersistantOptions).
    let capture = LogCapture::new();
    let shown = tracing::subscriber::with_default(capture.subscriber(), || f.get_options(&o));
    assert_eq!(shown, "(\"exponent\":\"3\",\"metric\":\"\")");
    let events = capture.events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].level(), Level::ERROR);
    assert_eq!(events[0].message(), Some("cannot cast to SimilarityType"));
    assert_eq!(events[0].field("value_type"), Some("int"));
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
    let f = t.index_factory().expect("hnsw is a factory");
    assert_eq!((f.name(), f.float_bits()), ("hnsw", 32));
    assert_eq!(t.type_name(), "float32vector");
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
