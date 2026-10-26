/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A vector index a predicate declares: its factory, `hnsw`, and the options it is built with, by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VectorIndexSpec {
    factory: String,
    options: Vec<(String, String)>,
}

impl VectorIndexSpec {
    pub fn new(factory: impl Into<String>, options: Vec<(String, String)>) -> Self {
        Self {
            factory: factory.into(),
            options,
        }
    }

    pub fn factory(&self) -> &str {
        &self.factory
    }

    pub fn options(&self) -> &[(String, String)] {
        &self.options
    }
}
