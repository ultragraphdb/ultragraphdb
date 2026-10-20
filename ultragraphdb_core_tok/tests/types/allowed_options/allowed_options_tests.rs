/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{
    AllowedOptions, OptionValue, OptionValuePair, Options, OptionsErrorEnum,
};

#[test]
fn test_allowed_options() {
    let mut ao = AllowedOptions::new();
    ao.add_int_option("i")
        .add_string_option("s")
        .add_float64_option("f");
    assert_eq!(ao.get_option_list(), vec!["f", "i", "s"]);
    assert_eq!(
        ao.get_parsed_option("i", "42").unwrap(),
        OptionValue::Int(42)
    );
    assert_eq!(
        ao.get_parsed_option("s", "x").unwrap(),
        OptionValue::Str("x".into())
    );
    assert_eq!(
        ao.get_parsed_option("f", "1.5").unwrap(),
        OptionValue::Float(1.5)
    );
    let err = ao.get_parsed_option("z", "1").unwrap_err();
    assert_eq!(err.to_string(), "option z is not allowed!");
    assert!(matches!(err.kind(), OptionsErrorEnum::NotAllowed(_)));

    let mut opts = Options::new();
    ao.populate_options(
        &[
            OptionValuePair::new("i", "1"),
            OptionValuePair::new("s", "t"),
        ],
        &mut opts,
    )
    .unwrap();
    assert_eq!(opts.get_opt::<i64>("i").unwrap(), Some(1));
    let err = ao
        .populate_options(&[OptionValuePair::new("i", "x")], &mut opts)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "strconv.Atoi: parsing \"x\": invalid syntax"
    );
    let pair = OptionValuePair::new("s", "v");
    assert_eq!((pair.option(), pair.value()), ("s", "v"));
    assert_eq!(ao.parse_pair(&pair).unwrap(), OptionValue::Str("v".into()));
}
