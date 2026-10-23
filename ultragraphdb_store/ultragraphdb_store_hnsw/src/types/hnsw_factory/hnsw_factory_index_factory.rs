/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_factory.go (Name, GetOptions, AllowedOptions, Create, Find,
 * Remove, CreateOrReplace). Changed for ultragraphdb: translated to Rust; no map of indexes is kept.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{HNSW, VectorIndexFactory, get_index_factory};
use ultragraphdb_traits_index::{AllowedOptions, IndexFactory, Options};

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::hnsw_factory::HnswFactory;
use crate::types::persistent_hnsw::PersistentHnsw;

/// The registry's entry for `hnsw`, which names the factory and parses and prints its options.
fn entry() -> Option<VectorIndexFactory> {
    get_index_factory(HNSW)
}

impl<T: HnswFloat> IndexFactory<T> for HnswFactory<T> {
    type Index = PersistentHnsw<T>;
    type Error = HnswError;

    fn name(&self) -> &str {
        HNSW
    }

    fn get_options(&self, o: &Options) -> String {
        entry().map(|f| f.get_options(o)).unwrap_or_default()
    }

    fn allowed_options(&self) -> AllowedOptions {
        entry().map(|f| f.allowed_options()).unwrap_or_default()
    }

    fn create(
        &self,
        name: &str,
        o: &Options,
        float_bits: usize,
    ) -> Result<Self::Index, Self::Error> {
        PersistentHnsw::new(name, o, float_bits)
    }

    fn find(&self, _: &str) -> Result<Option<Self::Index>, Self::Error> {
        Ok(None)
    }

    fn remove(&self, _: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn create_or_replace(
        &self,
        name: &str,
        o: &Options,
        float_bits: usize,
    ) -> Result<Self::Index, Self::Error> {
        self.create(name, o, float_bits)
    }
}
