/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{
    get_factory_create_spec_from_spec, get_index_factories_from_specs, get_index_factory,
    get_index_factory_opts_from_spec,
};
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

#[test]
fn test_hnsw_options_golden() {
    let cases: Vec<HnswCase> = cases("tok_hnsw_options");
    assert_eq!(cases.len(), 46);
    for c in cases {
        match get_factory_create_spec_from_spec(&c.factory, &c.options) {
            Ok(fcs) => {
                assert!(c.error.is_empty(), "{c:?}");
                assert_eq!(fcs.name(), c.name, "{c:?}");
                assert_eq!(fcs.factory().name(), "hnsw");
                assert_eq!(
                    get_index_factory_opts_from_spec(&c.factory, &c.options).unwrap(),
                    *fcs.opts()
                );
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
    // Go's GetIndexFactoriesFromSpecs: the found factories in spec order, an unknown name skipped.
    let names: Vec<String> = get_index_factories_from_specs(&["hnsw", "nosuch", "hnsw"])
        .iter()
        .map(|f| f.name().to_string())
        .collect();
    assert_eq!(names, vec!["hnsw", "hnsw"]);
    assert!(get_index_factories_from_specs(&[]).is_empty());
    assert_eq!(
        get_index_factory_opts_from_spec("nosuch", &[])
            .unwrap_err()
            .to_string(),
        "cannot get Options for factory named 'nosuch' (factory not found)"
    );
}
