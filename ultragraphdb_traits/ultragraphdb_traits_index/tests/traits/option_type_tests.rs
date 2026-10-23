/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_index::{Metric, OptionType, OptionValue};

#[test]
fn test_option_types_name_their_go_type() {
    assert_eq!(<i64 as OptionType>::GO_TYPE, "int");
    assert_eq!(<u64 as OptionType>::GO_TYPE, "uint");
    assert_eq!(<f64 as OptionType>::GO_TYPE, "float64");
    assert_eq!(<bool as OptionType>::GO_TYPE, "bool");
    assert_eq!(<String as OptionType>::GO_TYPE, "string");
}

#[test]
fn test_option_types_read_only_their_own_values() {
    assert_eq!(i64::from_option(&OptionValue::Int(-3)), Some(-3));
    assert_eq!(u64::from_option(&OptionValue::Uint(3)), Some(3));
    assert_eq!(f64::from_option(&OptionValue::Float(0.5)), Some(0.5));
    assert_eq!(bool::from_option(&OptionValue::Bool(true)), Some(true));
    assert_eq!(
        String::from_option(&OptionValue::Str("s".into())),
        Some("s".to_string())
    );
    let metric = OptionValue::Metric(Metric::Cosine);
    assert_eq!(i64::from_option(&metric), None);
    assert_eq!(u64::from_option(&OptionValue::Int(3)), None);
    assert_eq!(f64::from_option(&OptionValue::Int(3)), None);
    assert_eq!(bool::from_option(&OptionValue::Int(1)), None);
    assert_eq!(String::from_option(&metric), None);
}
