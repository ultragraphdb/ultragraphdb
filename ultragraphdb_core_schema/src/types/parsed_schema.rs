/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb;

/// Dgraph's `ParsedSchema`: the predicate and type updates of a schema text, in the order the text declares them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedSchema {
    preds: Vec<pb::SchemaUpdate>,
    types: Vec<pb::TypeUpdate>,
}

impl ParsedSchema {
    pub(crate) fn new(preds: Vec<pb::SchemaUpdate>, types: Vec<pb::TypeUpdate>) -> Self {
        Self { preds, types }
    }

    /// The predicate updates.
    pub fn preds(&self) -> &[pb::SchemaUpdate] {
        &self.preds
    }

    /// The type updates.
    pub fn types(&self) -> &[pb::TypeUpdate] {
        &self.types
    }

    /// The predicate and the type updates, by value.
    pub fn into_parts(self) -> (Vec<pb::SchemaUpdate>, Vec<pb::TypeUpdate>) {
        (self.preds, self.types)
    }
}
