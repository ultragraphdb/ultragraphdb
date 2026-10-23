/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/types.go (KeyValue). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A value a vector index writes under the key of an entity and an attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyValue {
    entity: u64,
    attr: String,
    value: Vec<u8>,
}

impl KeyValue {
    /// The value of `attr` of `entity`.
    pub fn new(entity: u64, attr: impl Into<String>, value: Vec<u8>) -> Self {
        Self {
            entity,
            attr: attr.into(),
            value,
        }
    }

    /// The uid of the key.
    pub fn entity(&self) -> u64 {
        self.entity
    }

    /// The attribute of the key.
    pub fn attr(&self) -> &str {
        &self.attr
    }

    /// The value.
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}
