/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_mapping::VectorIndexSpec;
use ultragraphdb_core_tok::get_index_factory;
use ultragraphdb_store_hnsw::{PersistentHnsw, VectorKeys};
use ultragraphdb_traits_index::Options;

use crate::errors::changeset_error::ChangesetError;

/// The HNSW index `spec` declares at `keys`, its options parsed by its factory. A factory the registry does not know,
/// and options it refuses, are refused naming the factory.
pub(crate) fn hnsw(
    spec: &VectorIndexSpec,
    keys: VectorKeys,
) -> Result<PersistentHnsw<f32>, ChangesetError> {
    let refused = |error: String| ChangesetError::VectorOption(spec.factory(), error);
    let factory = get_index_factory(spec.factory())
        .ok_or_else(|| refused("no such vector index".to_string()))?;
    let allowed = factory.allowed_options();
    let mut options = Options::new();
    for (name, value) in spec.options() {
        let parsed = allowed
            .get_parsed_option(name, value)
            .map_err(|e| refused(e.to_string()))?;
        options.set_opt(name.clone(), parsed);
    }
    PersistentHnsw::new(keys, &options, factory.float_bits()).map_err(|e| refused(e.to_string()))
}
