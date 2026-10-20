/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{
    TokErrorEnum, build_tokens, get_factory_create_spec_from_spec, get_tokenizer, get_tokenizers,
};
use ultragraphdb_core_types::Value;
use ultragraphdb_proto_pb::pb;

#[test]
fn test_tok_error_kinds() {
    let e = get_tokenizers(&["x"]).unwrap_err();
    assert!(matches!(e.kind(), TokErrorEnum::UnknownTokenizer(_)));
    let e = build_tokens(&Value::str("a"), &get_tokenizer("int").unwrap()).unwrap_err();
    assert!(matches!(e.kind(), TokErrorEnum::WrongValue(_)));
    assert_eq!(
        e.to_string(),
        "interface conversion: interface {} is string, not int64"
    );
    let line = ultragraphdb_core_types::Geometry::line_string(
        ultragraphdb_core_types::Layout::XY,
        &[vec![0.0, 0.0], vec![1.0, 1.0]],
    )
    .unwrap();
    let e = build_tokens(&Value::Geo(line), &get_tokenizer("geo").unwrap()).unwrap_err();
    assert!(matches!(e.kind(), TokErrorEnum::Geo(_)));
    assert_eq!(
        e.to_string(),
        "Cannot index geometry of type *geom.LineString"
    );
    let spec = pb::VectorIndexSpec {
        name: "hnsw".into(),
        options: vec![pb::OptionPair {
            key: "metric".into(),
            value: "l1".into(),
        }],
    };
    let e = get_factory_create_spec_from_spec(&spec).unwrap_err();
    assert!(matches!(e.kind(), TokErrorEnum::Options(_)));
    let e: &dyn std::error::Error = &e;
    assert_eq!(e.to_string(), "Can't create a vector index for l1");
}
