/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{
    FloatBits, TokError, TokErrorEnum, build_tokens, bytes_to_float,
    get_factory_create_spec_from_spec, get_index_factory_opts_from_spec, get_tokenizer,
    get_tokenizers, get_tokens,
};
use ultragraphdb_core_types::{Geometry, Layout, Value};
use ultragraphdb_tool_bdd::{Context, TestResult};
/// A spec's option pairs as the factory lookups take them.
fn options(options: &[(&str, &str)]) -> Vec<(String, String)> {
    options
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}

/// Each error the API returns, its variant and its Dgraph text.
#[test]
fn test_tok_error_kinds() -> TestResult {
    let e = get_tokenizers(&["x"]).err().context("expected an error")?;
    assert_eq!(
        e.0,
        TokErrorEnum::UnknownTokenizer("Invalid tokenizer x".into())
    );
    assert_eq!(e.to_string(), "Invalid tokenizer x");
    let int_err = build_tokens(
        &Value::str("a"),
        &get_tokenizer("int").context("get_tokenizer(\"int\")")?,
    )
    .err()
    .context("expected an error")?;
    assert!(matches!(int_err.0, TokErrorEnum::WrongValue(_)));
    assert_eq!(
        int_err.to_string(),
        "interface conversion: interface {} is string, not int64"
    );
    let line = Geometry::line_string(Layout::XY, &[vec![0.0, 0.0], vec![1.0, 1.0]])?;
    let geo_err = build_tokens(
        &Value::Geo(line),
        &get_tokenizer("geo").context("get_tokenizer(\"geo\")")?,
    )
    .err()
    .context("expected an error")?;
    assert!(matches!(geo_err.0, TokErrorEnum::Geo(_)));
    assert_eq!(
        geo_err.to_string(),
        "Cannot index geometry of type *geom.LineString"
    );
    Ok(())
}

/// Each error of token functions, index factories and float decoding, its variant and its Dgraph text.
#[test]
fn test_tok_error_kinds_of_functions_and_factories() -> TestResult {
    let tokens_err = get_tokens(0x01, &["a", "b"])
        .err()
        .context("expected an error")?;
    assert_eq!(
        tokens_err.0,
        TokErrorEnum::Arguments("Function requires 1 arguments, but got 2".into())
    );
    let factory_err = get_factory_create_spec_from_spec("nosuch", &[])
        .err()
        .context("expected an error")?;
    assert_eq!(
        factory_err.0,
        TokErrorEnum::UnknownFactory("cannot find index factory named 'nosuch'".into())
    );
    assert_eq!(
        factory_err.to_string(),
        "cannot find index factory named 'nosuch'"
    );
    let opts_err = get_index_factory_opts_from_spec("", &[])
        .err()
        .context("expected an error")?;
    assert_eq!(
        opts_err.0,
        TokErrorEnum::UnknownFactory(
            "cannot get Options for factory named '' (factory not found)".into()
        )
    );
    let metric_err = get_factory_create_spec_from_spec("hnsw", &options(&[("metric", "l1")]))
        .err()
        .context("expected an error")?;
    assert!(matches!(metric_err.0, TokErrorEnum::Options(_)));
    let e: &dyn std::error::Error = &metric_err;
    assert_eq!(e.to_string(), "Can't create a vector index for l1");
    assert!(e.source().is_none());
    let float_err = bytes_to_float::<f32>(&[1], FloatBits::F32)
        .err()
        .context("expected an error")?;
    assert_eq!(float_err.0, TokErrorEnum::ShortFloat { len: 1, bits: 32 });
    assert_eq!(
        float_err.to_string(),
        "cannot read a 32-bit float from 1 bytes"
    );
    Ok(())
}

/// The public constructors build the variant they are named after, and `Display` prints it; the wrapped errors of
/// other crates print as those crates do, by constructor and by `From` alike.
#[test]
fn test_tok_error_constructors() -> TestResult {
    let cases = [
        (
            TokError::UnknownTokenizer("Invalid tokenizer t".into()),
            TokErrorEnum::UnknownTokenizer("Invalid tokenizer t".into()),
            "Invalid tokenizer t",
        ),
        (
            TokError::WrongValue("Exact indices only supported for string types".into()),
            TokErrorEnum::WrongValue("Exact indices only supported for string types".into()),
            "Exact indices only supported for string types",
        ),
        (
            TokError::Arguments("Function requires 1 arguments, but got 0".into()),
            TokErrorEnum::Arguments("Function requires 1 arguments, but got 0".into()),
            "Function requires 1 arguments, but got 0",
        ),
        (
            TokError::UnknownFactory("cannot find index factory named 'x'".into()),
            TokErrorEnum::UnknownFactory("cannot find index factory named 'x'".into()),
            "cannot find index factory named 'x'",
        ),
        (
            TokError::ShortFloat(0, 64),
            TokErrorEnum::ShortFloat { len: 0, bits: 64 },
            "cannot read a 64-bit float from 0 bytes",
        ),
        (
            TokError::UnknownTokenizer(String::new()),
            TokErrorEnum::UnknownTokenizer(String::new()),
            "",
        ),
    ];
    for (e, kind, text) in cases {
        assert_eq!(e.0, kind);
        assert_eq!(e.to_string(), text);
    }

    let line = Geometry::line_string(Layout::XY, &[vec![0.0, 0.0], vec![1.0, 1.0]])?;
    let error = build_tokens(
        &Value::Geo(line),
        &get_tokenizer("geo").context("get_tokenizer(\"geo\")")?,
    )
    .err()
    .context("expected an error")?;
    let types = if let TokErrorEnum::Geo(types) = error.0 {
        Some(types)
    } else {
        None
    }
    .context("a geo error")?;
    for e in [TokError::Geo(types.clone()), TokError::from(types.clone())] {
        assert_eq!(e.0, TokErrorEnum::Geo(types.clone()));
        assert_eq!(e.to_string(), types.to_string());
    }
    let ef_search_err = get_factory_create_spec_from_spec("hnsw", &options(&[("efSearch", "x")]))
        .err()
        .context("expected an error")?;
    let options = if let TokErrorEnum::Options(options) = ef_search_err.0 {
        Some(options)
    } else {
        None
    }
    .context("an options error")?;
    for e in [
        TokError::Options(options.clone()),
        TokError::from(options.clone()),
    ] {
        assert_eq!(e.0, TokErrorEnum::Options(options.clone()));
        assert_eq!(e.to_string(), options.to_string());
    }
    Ok(())
}
