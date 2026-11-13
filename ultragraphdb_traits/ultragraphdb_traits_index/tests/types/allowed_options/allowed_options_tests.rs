/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_options::OptionsErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_index::{AllowedOptions, OptionValue, OptionValuePair, Options};

/// The standard library's message for refusing `text` as a `T`.
fn refused<T>(text: &str) -> String
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    text.parse::<T>()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

#[test]
fn test_allowed_options() -> TestResult {
    let mut ao = AllowedOptions::new();
    let _add_float64_option: &mut AllowedOptions = ao
        .add_int_option("i")
        .add_string_option("s")
        .add_float64_option("f");
    assert_eq!(ao.get_option_list(), vec!["f", "i", "s"]);
    assert_eq!(ao.get_parsed_option("i", "42")?, OptionValue::Int(42));
    assert_eq!(
        ao.get_parsed_option("s", "x")?,
        OptionValue::Str("x".into())
    );
    assert_eq!(ao.get_parsed_option("f", "1.5")?, OptionValue::Float(1.5));
    let err = ao
        .get_parsed_option("z", "1")
        .err()
        .context("the error of ao.get_parsed_option(\"z\", \"1\")")?;
    assert_eq!(err.to_string(), "option z is not allowed!");
    assert!(matches!(err.0, OptionsErrorEnum::NotAllowed(_)));

    let mut opts = Options::new();
    ao.populate_options(
        &[
            OptionValuePair::new("i", "1"),
            OptionValuePair::new("s", "t"),
        ],
        &mut opts,
    )?;
    assert_eq!(opts.get_opt::<i64>("i")?, Some(1));
    let populate_err = ao
        .populate_options(&[OptionValuePair::new("i", "x")], &mut opts)
        .err()
        .context("the error of populate_options(..)")?;
    assert_eq!(populate_err.to_string(), refused::<i64>("x"));
    let pair = OptionValuePair::new("s", "v");
    assert_eq!((pair.option(), pair.value()), ("s", "v"));
    assert_eq!(ao.parse_pair(&pair)?, OptionValue::Str("v".into()));
    Ok(())
}
