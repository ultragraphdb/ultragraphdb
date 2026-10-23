/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/mutation.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb;

/// The N-Quads of a mutation's set and delete operations, its condition, and what an ACL-enabled request may
/// write.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mutation {
    cond: String,
    set: Vec<api::NQuad>,
    del: Vec<api::NQuad>,
    allowed_preds: Vec<String>,
    metadata: Option<pb::Metadata>,
}

impl Mutation {
    /// A mutation of N-Quads to set and to delete, under a condition (`@if(...)`, empty for none).
    pub fn new(cond: impl Into<String>, set: Vec<api::NQuad>, del: Vec<api::NQuad>) -> Self {
        Self {
            cond: cond.into(),
            set,
            del,
            ..Self::default()
        }
    }

    pub fn cond(&self) -> &str {
        &self.cond
    }

    pub fn set(&self) -> &[api::NQuad] {
        &self.set
    }

    pub fn del(&self) -> &[api::NQuad] {
        &self.del
    }

    pub fn allowed_preds(&self) -> &[String] {
        &self.allowed_preds
    }

    pub fn metadata(&self) -> Option<&pb::Metadata> {
        self.metadata.as_ref()
    }
}
