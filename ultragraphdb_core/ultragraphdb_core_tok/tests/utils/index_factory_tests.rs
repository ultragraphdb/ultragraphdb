/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{
    get_factory_create_spec_from_spec, get_index_factories_from_specs, get_index_factory,
    get_index_factory_opts_from_spec,
};
use ultragraphdb_proto_pb::pb;
use ultragraphdb_traits_index::OptionValue;

use crate::golden::cases;

#[derive(Debug, Clone, Deserialize)]
struct Parsed {
    value: String,
    #[serde(rename = "type")]
    typ: String,
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
struct HnswCase {
    factory: String,
    options: Vec<(String, String)>,
    name: String,
    error: String,
    parsed: Vec<Parsed>,
}

fn spec(name: &str, options: &[(String, String)]) -> pb::VectorIndexSpec {
    pb::VectorIndexSpec {
        name: name.to_string(),
        options: options
            .iter()
            .map(|(k, v)| pb::OptionPair {
                key: k.clone(),
                value: v.clone(),
            })
            .collect(),
    }
}

#[test]
fn test_hnsw_options_golden() {
    let cases: Vec<HnswCase> = cases("tok_hnsw_options");
    assert_eq!(cases.len(), 46);
    for c in cases {
        let s = spec(&c.factory, &c.options);
        match get_factory_create_spec_from_spec(&s) {
            Ok(fcs) => {
                assert!(c.error.is_empty(), "{c:?}");
                assert_eq!(fcs.name(), c.name, "{c:?}");
                assert_eq!(fcs.factory().name(), "hnsw");
                assert_eq!(get_index_factory_opts_from_spec(&s).unwrap(), *fcs.opts());
            }
            Err(e) => assert_eq!(e.to_string(), c.error, "{c:?}"),
        }
        let Some(f) = get_index_factory(&c.factory) else {
            assert!(c.parsed.is_empty());
            continue;
        };
        let ao = f.allowed_options();
        for ((k, v), p) in c.options.iter().zip(&c.parsed) {
            match ao.get_parsed_option(k, v) {
                Ok(val) => {
                    assert_eq!(val.go_type(), p.typ, "{k}={v}");
                    let shown = match &val {
                        OptionValue::Int(i) => i.to_string(),
                        OptionValue::Metric(m) => m.name().to_string(),
                        other => panic!("{other:?}"),
                    };
                    assert_eq!(shown, p.value, "{k}={v}");
                }
                Err(e) => assert_eq!(e.to_string(), p.error, "{k}={v}"),
            }
        }
    }
}

#[test]
fn test_factory_lookups_from_specs() {
    let none: Vec<(String, String)> = Vec::new();
    let specs = [
        spec("hnsw", &none),
        spec("nosuch", &none),
        spec("hnsw", &none),
    ];
    assert_eq!(get_index_factories_from_specs(&specs).len(), 2);
    assert_eq!(
        get_index_factory_opts_from_spec(&specs[1])
            .unwrap_err()
            .to_string(),
        "cannot get Options for factory named 'nosuch' (factory not found)"
    );
}
