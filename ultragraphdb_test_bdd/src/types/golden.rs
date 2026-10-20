/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;

/// A golden-vector file: the Dgraph commit it was generated from, the generator, and its cases.
#[derive(Debug, Clone, Deserialize)]
pub struct Golden<T> {
    dgraph_commit: String,
    generator: String,
    cases: Vec<T>,
}

impl<T> Golden<T> {
    pub fn dgraph_commit(&self) -> &str {
        &self.dgraph_commit
    }

    pub fn generator(&self) -> &str {
        &self.generator
    }

    pub fn cases(&self) -> &[T] {
        &self.cases
    }
}
