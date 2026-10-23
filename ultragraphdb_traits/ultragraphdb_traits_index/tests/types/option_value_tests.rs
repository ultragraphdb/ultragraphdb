/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_index::{Metric, OptionValue};

#[test]
fn test_option_values_name_their_go_type() {
    assert_eq!(OptionValue::Int(1).go_type(), "int");
    assert_eq!(OptionValue::Uint(1).go_type(), "uint");
    assert_eq!(OptionValue::Float(1.0).go_type(), "float64");
    assert_eq!(OptionValue::Bool(true).go_type(), "bool");
    assert_eq!(OptionValue::Str("s".into()).go_type(), "string");
    assert_eq!(
        OptionValue::Metric(Metric::Euclidean).go_type(),
        "hnsw.SimilarityType[float32]"
    );
}
