/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index_factory.go (FactoryCreateSpec). Changed for ultragraphdb: translated to
 * Rust; `CreateIndex` comes with the index in stage 2.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::options::Options;
use crate::types::vector_index_factory::VectorIndexFactory;

/// Dgraph's `FactoryCreateSpec`: a vector index factory and the options to create an index with.
#[derive(Debug, Clone, PartialEq)]
pub struct FactoryCreateSpec {
    factory: VectorIndexFactory,
    opts: Options,
}

impl FactoryCreateSpec {
    pub(crate) fn new(factory: VectorIndexFactory, opts: Options) -> Self {
        Self { factory, opts }
    }

    /// Dgraph's `Name`: the factory's name and its options, e.g. `hnsw("metric":"cosine")`.
    pub fn name(&self) -> String {
        format!(
            "{}{}",
            self.factory.name(),
            self.factory.get_options(&self.opts)
        )
    }

    /// The factory.
    pub fn factory(&self) -> &VectorIndexFactory {
        &self.factory
    }

    /// The parsed options.
    pub fn opts(&self) -> &Options {
        &self.opts
    }
}
