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
use ultragraphdb_proto_pb::pb;

fn spec(name: &str, options: &[(&str, &str)]) -> pb::VectorIndexSpec {
    pb::VectorIndexSpec {
        name: name.into(),
        options: options
            .iter()
            .map(|(k, v)| pb::OptionPair {
                key: (*k).into(),
                value: (*v).into(),
            })
            .collect(),
    }
}

/// Each error the API returns, its variant and its Dgraph text.
#[test]
fn test_tok_error_kinds() {
    let e = get_tokenizers(&["x"]).unwrap_err();
    assert_eq!(
        e.0,
        TokErrorEnum::UnknownTokenizer("Invalid tokenizer x".into())
    );
    assert_eq!(e.to_string(), "Invalid tokenizer x");
    let e = build_tokens(&Value::str("a"), &get_tokenizer("int").unwrap()).unwrap_err();
    assert!(matches!(e.0, TokErrorEnum::WrongValue(_)));
    assert_eq!(
        e.to_string(),
        "interface conversion: interface {} is string, not int64"
    );
    let line = Geometry::line_string(Layout::XY, &[vec![0.0, 0.0], vec![1.0, 1.0]]).unwrap();
    let e = build_tokens(&Value::Geo(line), &get_tokenizer("geo").unwrap()).unwrap_err();
    assert!(matches!(e.0, TokErrorEnum::Geo(_)));
    assert_eq!(
        e.to_string(),
        "Cannot index geometry of type *geom.LineString"
    );
    let e = get_tokens(0x01, &["a", "b"]).unwrap_err();
    assert_eq!(
        e.0,
        TokErrorEnum::Arguments("Function requires 1 arguments, but got 2".into())
    );
    let e = get_factory_create_spec_from_spec(&spec("nosuch", &[])).unwrap_err();
    assert_eq!(
        e.0,
        TokErrorEnum::UnknownFactory("cannot find index factory named 'nosuch'".into())
    );
    assert_eq!(e.to_string(), "cannot find index factory named 'nosuch'");
    let e = get_index_factory_opts_from_spec(&spec("", &[])).unwrap_err();
    assert_eq!(
        e.0,
        TokErrorEnum::UnknownFactory(
            "cannot get Options for factory named '' (factory not found)".into()
        )
    );
    let e = get_factory_create_spec_from_spec(&spec("hnsw", &[("metric", "l1")])).unwrap_err();
    assert!(matches!(e.0, TokErrorEnum::Options(_)));
    let e: &dyn std::error::Error = &e;
    assert_eq!(e.to_string(), "Can't create a vector index for l1");
    assert!(e.source().is_none());
    let e = bytes_to_float::<f32>(&[1], FloatBits::F32).unwrap_err();
    assert_eq!(e.0, TokErrorEnum::ShortFloat { len: 1, bits: 32 });
    assert_eq!(e.to_string(), "cannot read a 32-bit float from 1 bytes");
}

/// The public constructors build the variant they are named after, and `Display` prints it; the wrapped errors of
/// other crates print as those crates do, by constructor and by `From` alike.
#[test]
fn test_tok_error_constructors() {
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

    let line = Geometry::line_string(Layout::XY, &[vec![0.0, 0.0], vec![1.0, 1.0]]).unwrap();
    let TokErrorEnum::Geo(types) = build_tokens(&Value::Geo(line), &get_tokenizer("geo").unwrap())
        .unwrap_err()
        .0
    else {
        panic!("a geo error");
    };
    for e in [TokError::Geo(types.clone()), TokError::from(types.clone())] {
        assert_eq!(e.0, TokErrorEnum::Geo(types.clone()));
        assert_eq!(e.to_string(), types.to_string());
    }
    let TokErrorEnum::Options(options) =
        get_factory_create_spec_from_spec(&spec("hnsw", &[("efSearch", "x")]))
            .unwrap_err()
            .0
    else {
        panic!("an options error");
    };
    for e in [
        TokError::Options(options.clone()),
        TokError::from(options.clone()),
    ] {
        assert_eq!(e.0, TokErrorEnum::Options(options.clone()));
        assert_eq!(e.to_string(), options.to_string());
    }
}
