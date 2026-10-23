/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/options_test.go. Changed for ultragraphdb: translated to Rust; the
 * interface value Go stores is a metric here, the one non-simple value options hold.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::{Metric, OptionValue, Options};

#[test]
fn test_simple_options() {
    let mut o = Options::new();
    o.set_opt("fish", OptionValue::Int(17));
    // Found, of another type: Go returns the default 32.2, true and an error.
    let err = o.get_opt::<f64>("fish").unwrap_err();
    assert_eq!(
        err.to_string(),
        "unable to assign a value of type int to type float64"
    );
    assert_eq!(o.get_opt::<i64>("fish").unwrap(), Some(17));
    // Not found: Go returns the default "fishcat", false and no error.
    assert_eq!(o.get_opt::<String>("cat").unwrap(), None);
    assert!(o.specifies("fish"));
    assert!(!o.specifies("cat"));
}

#[test]
fn test_interface_options() {
    let mut o = Options::new();
    o.set_opt("foob", OptionValue::Metric(Metric::Cosine));
    assert_eq!(
        o.get_interface_opt("foob"),
        Some(&OptionValue::Metric(Metric::Cosine))
    );
    assert_eq!(o.get_interface_opt("barb"), None);
}

#[test]
fn test_option_types() {
    let mut o = Options::new();
    o.set_opt("u", OptionValue::Uint(3))
        .set_opt("b", OptionValue::Bool(true))
        .set_opt("s", OptionValue::Str("x".into()));
    assert_eq!(o.get_opt::<u64>("u").unwrap(), Some(3));
    assert_eq!(o.get_opt::<bool>("b").unwrap(), Some(true));
    assert_eq!(o.get_opt::<String>("s").unwrap(), Some("x".to_string()));
    assert_eq!(
        o.get_opt::<i64>("s").unwrap_err().to_string(),
        "unable to assign a value of type string to type int"
    );
    assert_eq!(
        OptionValue::Metric(Metric::Euclidean).go_type(),
        "hnsw.SimilarityType[float32]"
    );
    assert_eq!(OptionValue::Float(1.0).go_type(), "float64");
}
