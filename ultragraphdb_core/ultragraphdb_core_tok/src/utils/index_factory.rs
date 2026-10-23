/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok.go (the vector index factory lookups). Changed for ultragraphdb: translated
 * to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb;
use ultragraphdb_traits_index::Options;

use crate::errors::tok_error::TokError;
use crate::types::factory_create_spec::FactoryCreateSpec;
use crate::types::vector_index_factory::VectorIndexFactory;
use crate::utils::hnsw::HNSW;

/// Dgraph's `GetIndexFactory`: the vector index factory registered under `name`, only `hnsw`.
pub fn get_index_factory(name: &str) -> Option<VectorIndexFactory> {
    (name == HNSW).then(VectorIndexFactory::hnsw)
}

/// Dgraph's `GetIndexFactoryFromSpec`.
pub fn get_index_factory_from_spec(spec: &pb::VectorIndexSpec) -> Option<VectorIndexFactory> {
    get_index_factory(&spec.name)
}

/// Dgraph's `getOptsFromFactorySpec`: the spec's options, each parsed by the factory.
fn opts_from_factory_spec(
    f: &VectorIndexFactory,
    spec: &pb::VectorIndexSpec,
) -> Result<Options, TokError> {
    let allowed = f.allowed_options();
    let mut opts = Options::new();
    for pair in &spec.options {
        let v = allowed.get_parsed_option(&pair.key, &pair.value)?;
        opts.set_opt(pair.key.clone(), v);
    }
    Ok(opts)
}

/// Dgraph's `GetFactoryCreateSpecFromSpec`: the factory a spec names with its parsed options.
pub fn get_factory_create_spec_from_spec(
    spec: &pb::VectorIndexSpec,
) -> Result<FactoryCreateSpec, TokError> {
    let factory = get_index_factory_from_spec(spec).ok_or_else(|| {
        TokError::unknown_factory(format!("cannot find index factory named '{}'", spec.name))
    })?;
    let opts = opts_from_factory_spec(&factory, spec)?;
    Ok(FactoryCreateSpec::new(factory, opts))
}

/// Dgraph's `GetIndexFactoryOptsFromSpec`: the parsed options of a spec.
pub fn get_index_factory_opts_from_spec(spec: &pb::VectorIndexSpec) -> Result<Options, TokError> {
    let factory = get_index_factory_from_spec(spec).ok_or_else(|| {
        TokError::unknown_factory(format!(
            "cannot get Options for factory named '{}' (factory not found)",
            spec.name
        ))
    })?;
    opts_from_factory_spec(&factory, spec)
}

/// Dgraph's `GetIndexFactoriesFromSpecs`: the factories the specs name, unknown names skipped.
pub fn get_index_factories_from_specs(specs: &[pb::VectorIndexSpec]) -> Vec<VectorIndexFactory> {
    specs
        .iter()
        .filter_map(get_index_factory_from_spec)
        .collect()
}
