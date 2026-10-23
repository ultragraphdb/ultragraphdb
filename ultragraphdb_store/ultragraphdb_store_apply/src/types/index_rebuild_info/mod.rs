/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (indexRebuildInfo, appendTokenizerPrefixesToDelete,
 * prefixesForVectorIndexes, prefixesForTokIndexes). Changed for ultragraphdb: translated to Rust; the prefixes of a
 * vector index whose name does not resolve are an error, where Dgraph returns none and drops nothing.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::VectorIndexSpec;

use crate::errors::apply_error::ApplyError;
use crate::types::index_op::IndexOp;
use crate::utils::prefixes::tokenizer_prefixes;

/// What a schema change does to a predicate's tokenizer and vector indexes: Dgraph's `indexRebuildInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexRebuildInfo {
    op: IndexOp,
    attr: String,
    tokenizers_to_delete: Vec<String>,
    tokenizers_to_rebuild: Vec<String>,
    vector_indexes_to_delete: Vec<VectorIndexSpec>,
    vector_indexes_to_rebuild: Vec<VectorIndexSpec>,
}

impl IndexRebuildInfo {
    pub(crate) fn new(
        op: IndexOp,
        attr: &str,
        tokenizers: (Vec<String>, Vec<String>),
        vector_indexes: (Vec<VectorIndexSpec>, Vec<VectorIndexSpec>),
    ) -> Self {
        Self {
            op,
            attr: attr.to_string(),
            tokenizers_to_delete: tokenizers.0,
            tokenizers_to_rebuild: tokenizers.1,
            vector_indexes_to_delete: vector_indexes.0,
            vector_indexes_to_rebuild: vector_indexes.1,
        }
    }

    /// Nothing to do.
    pub(crate) fn noop(attr: &str) -> Self {
        Self::new(IndexOp::Noop, attr, Default::default(), Default::default())
    }

    pub fn op(&self) -> IndexOp {
        self.op
    }

    pub fn attr(&self) -> &str {
        &self.attr
    }

    pub fn tokenizers_to_delete(&self) -> &[String] {
        &self.tokenizers_to_delete
    }

    pub fn tokenizers_to_rebuild(&self) -> &[String] {
        &self.tokenizers_to_rebuild
    }

    pub fn vector_indexes_to_delete(&self) -> &[VectorIndexSpec] {
        &self.vector_indexes_to_delete
    }

    pub fn vector_indexes_to_rebuild(&self) -> &[VectorIndexSpec] {
        &self.vector_indexes_to_rebuild
    }

    /// Dgraph's `prefixesForTokIndexes`: the prefixes of the tokenizers deleted and of those rebuilt, each rebuilt
    /// index deleted before it is built; none for nothing to do.
    pub fn prefixes_for_tok_indexes(&self) -> Result<Vec<Vec<u8>>, ApplyError> {
        if self.op == IndexOp::Noop {
            return Ok(Vec::new());
        }
        let mut prefixes = Vec::new();
        for t in self
            .tokenizers_to_delete
            .iter()
            .chain(&self.tokenizers_to_rebuild)
        {
            prefixes.extend(tokenizer_prefixes(&self.attr, t)?);
        }
        Ok(prefixes)
    }

    /// Dgraph's `prefixesForVectorIndexes`: the prefixes of the vector indexes deleted and rebuilt, named as
    /// tokenizers are.
    pub fn prefixes_for_vector_indexes(&self) -> Result<Vec<Vec<u8>>, ApplyError> {
        if self.op == IndexOp::Noop {
            return Ok(Vec::new());
        }
        let mut prefixes = Vec::new();
        for spec in self
            .vector_indexes_to_delete
            .iter()
            .chain(&self.vector_indexes_to_rebuild)
        {
            prefixes.extend(tokenizer_prefixes(&self.attr, &spec.name)?);
        }
        Ok(prefixes)
    }

    /// The prefixes of the tokenizers deleted and not rebuilt.
    pub(crate) fn removed_tokenizer_prefixes(&self) -> Result<Vec<Vec<u8>>, ApplyError> {
        let mut prefixes = Vec::new();
        for t in self
            .tokenizers_to_delete
            .iter()
            .filter(|t| !self.tokenizers_to_rebuild.contains(t))
        {
            prefixes.extend(tokenizer_prefixes(&self.attr, t)?);
        }
        Ok(prefixes)
    }

    /// The prefixes of the tokenizers rebuilt.
    pub(crate) fn rebuilt_tokenizer_prefixes(&self) -> Result<Vec<Vec<u8>>, ApplyError> {
        let mut prefixes = Vec::new();
        for t in &self.tokenizers_to_rebuild {
            prefixes.extend(tokenizer_prefixes(&self.attr, t)?);
        }
        Ok(prefixes)
    }
}
