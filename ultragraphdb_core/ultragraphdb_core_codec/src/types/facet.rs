/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/api.proto (Facet). Changed for ultragraphdb: a Rust type in place of the
 * protobuf message, carrying the key alone, the one field any code reads.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A facet of a posting: Dgraph's `api.Facet`, of which only the key is carried.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Facet {
    key: String,
}

impl Facet {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}
