/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (IndexRebuild, GetQuerySchema, DropIndexes' prefixes,
 * NeedIndexRebuild, needsTokIndexRebuild, needsCountIndexRebuild, needsVectorIndexEdgesRebuild,
 * needsReverseEdgesRebuild, needsListTypeRebuild). Changed for ultragraphdb: translated to Rust; a vector index
 * specification that does not resolve is an error, where Dgraph asserts; the tokenizers and vector indexes added and
 * removed are in the order the schemas name them, where Dgraph's `x.Diff` iterates maps in random order.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::get_factory_create_spec_from_spec;
use ultragraphdb_proto_pb::pb::schema_update::Directive;
use ultragraphdb_proto_pb::pb::{SchemaUpdate, VectorIndexSpec, posting};

use crate::errors::apply_error::ApplyError;
use crate::types::index_op::IndexOp;
use crate::types::index_rebuild_info::IndexRebuildInfo;
use crate::utils::prefixes::{count_prefixes, reverse_prefixes, vector_edge_prefixes};
use crate::utils::validate::attr_name;

/// A schema change of one predicate and the indexes it rebuilds: Dgraph's `IndexRebuild`. `old_schema` is the
/// predicate's schema before the change, `None` for a predicate without one.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexRebuild {
    attr: String,
    start_ts: u64,
    old_schema: Option<SchemaUpdate>,
    current_schema: SchemaUpdate,
}

impl IndexRebuild {
    pub fn new(
        attr: &str,
        start_ts: u64,
        old_schema: Option<SchemaUpdate>,
        current_schema: SchemaUpdate,
    ) -> Self {
        Self {
            attr: attr.to_string(),
            start_ts,
            old_schema,
            current_schema,
        }
    }

    pub fn attr(&self) -> &str {
        &self.attr
    }

    /// The change's timestamp: the indexes are built from the data at it.
    pub fn start_ts(&self) -> u64 {
        self.start_ts
    }

    pub fn old_schema(&self) -> Option<&SchemaUpdate> {
        self.old_schema.as_ref()
    }

    pub fn current_schema(&self) -> &SchemaUpdate {
        &self.current_schema
    }

    /// The change at another timestamp.
    pub(crate) fn at(&self, start_ts: u64) -> Self {
        Self {
            start_ts,
            ..self.clone()
        }
    }

    /// The old schema, an empty one for a predicate without one.
    fn old(&self) -> SchemaUpdate {
        self.old_schema.clone().unwrap_or_default()
    }

    /// Dgraph's `GetQuerySchema`: the schema queries use while indexes are built: the new schema, with the old
    /// tokenizers less those the change deletes, and without the count index or reverse edges being built.
    pub fn get_query_schema(&self) -> Result<SchemaUpdate, ApplyError> {
        let mut query = self.current_schema.clone();
        let info = self.needs_tok_index_rebuild()?;
        query.tokenizer = self
            .old()
            .tokenizer
            .into_iter()
            .filter(|t| !info.tokenizers_to_delete().contains(t))
            .collect();
        if self.needs_count_index_rebuild() == IndexOp::Rebuild {
            query.count = false;
        }
        if self.needs_reverse_edges_rebuild() == IndexOp::Rebuild {
            query.directive = Directive::None as i32;
        }
        Ok(query)
    }

    /// The prefixes Dgraph's `DropIndexes` drops before the indexes are built: every index deleted or rebuilt.
    pub fn prefixes_to_drop(&self) -> Result<Vec<Vec<u8>>, ApplyError> {
        let info = self.needs_tok_index_rebuild()?;
        let mut prefixes = info.prefixes_for_tok_indexes()?;
        prefixes.extend(info.prefixes_for_vector_indexes()?);
        if self.needs_reverse_edges_rebuild() != IndexOp::Noop {
            prefixes.extend(reverse_prefixes(&self.attr)?);
        }
        if self.needs_count_index_rebuild() != IndexOp::Noop {
            prefixes.extend(count_prefixes(&self.attr)?);
        }
        if self.needs_vector_index_edges_rebuild()? != IndexOp::Noop {
            prefixes.extend(vector_edge_prefixes(&self.attr)?);
        }
        Ok(prefixes)
    }

    /// Dgraph's `NeedIndexRebuild`: whether a tokenizer, reverse or count index is built.
    pub fn need_index_rebuild(&self) -> Result<bool, ApplyError> {
        Ok(self.needs_tok_index_rebuild()?.op() == IndexOp::Rebuild
            || self.needs_reverse_edges_rebuild() == IndexOp::Rebuild
            || self.needs_count_index_rebuild() == IndexOp::Rebuild)
    }

    /// Dgraph's `needsTokIndexRebuild`: the tokenizers and vector indexes the change deletes and builds. A change of
    /// the value type rebuilds every one.
    ///
    /// # Errors
    ///
    /// A vector index specification the registry refuses; Dgraph asserts.
    pub fn needs_tok_index_rebuild(&self) -> Result<IndexRebuildInfo, ApplyError> {
        let old = self.old();
        let current = &self.current_schema;
        let curr_index = current.directive == Directive::Index as i32;
        let prev_index = old.directive == Directive::Index as i32;
        if !curr_index && !prev_index {
            return Ok(IndexRebuildInfo::noop(&self.attr));
        }
        if !curr_index {
            return Ok(IndexRebuildInfo::new(
                IndexOp::Delete,
                &self.attr,
                (old.tokenizer, Vec::new()),
                (old.index_specs, Vec::new()),
            ));
        }
        if current.value_type != old.value_type {
            return Ok(IndexRebuildInfo::new(
                IndexOp::Rebuild,
                &self.attr,
                (old.tokenizer, current.tokenizer.clone()),
                (old.index_specs, current.index_specs.clone()),
            ));
        }
        let new_tokenizers = diff(&current.tokenizer, &old.tokenizer);
        let deleted_tokenizers = diff(&old.tokenizer, &current.tokenizer);
        let (new_specs, deleted_specs) = spec_diff(&current.index_specs, &old.index_specs)?;
        if new_tokenizers.is_empty()
            && deleted_tokenizers.is_empty()
            && new_specs.is_empty()
            && deleted_specs.is_empty()
        {
            return Ok(IndexRebuildInfo::noop(&self.attr));
        }
        Ok(IndexRebuildInfo::new(
            IndexOp::Rebuild,
            &self.attr,
            (deleted_tokenizers, new_tokenizers),
            (deleted_specs, new_specs),
        ))
    }

    /// Dgraph's `needsCountIndexRebuild`.
    pub fn needs_count_index_rebuild(&self) -> IndexOp {
        let old = self.old();
        if self.current_schema.count == old.count {
            IndexOp::Noop
        } else if !self.current_schema.count {
            IndexOp::Delete
        } else {
            IndexOp::Rebuild
        }
    }

    /// Dgraph's `needsVectorIndexEdgesRebuild`: what happens to the predicates a vector index keeps.
    ///
    /// # Errors
    ///
    /// A vector index specification the registry refuses; Dgraph asserts.
    pub fn needs_vector_index_edges_rebuild(&self) -> Result<IndexOp, ApplyError> {
        let old = self.old();
        let vector = |s: &SchemaUpdate| {
            s.directive == Directive::Index as i32
                && s.value_type == posting::ValType::Vfloat as i32
        };
        let (curr_index, prev_index) = (vector(&self.current_schema), vector(&old));
        let (new_specs, deleted_specs) =
            spec_diff(&self.current_schema.index_specs, &old.index_specs)?;
        if curr_index == prev_index && new_specs.is_empty() && deleted_specs.is_empty() {
            return Ok(IndexOp::Noop);
        }
        if curr_index || !new_specs.is_empty() {
            return Ok(IndexOp::Rebuild);
        }
        Ok(IndexOp::Delete)
    }

    /// Dgraph's `needsReverseEdgesRebuild`.
    pub fn needs_reverse_edges_rebuild(&self) -> IndexOp {
        let reverse = |s: &SchemaUpdate| s.directive == Directive::Reverse as i32;
        match (reverse(&self.current_schema), reverse(&self.old())) {
            (c, p) if c == p => IndexOp::Noop,
            (true, _) => IndexOp::Rebuild,
            _ => IndexOp::Delete,
        }
    }

    /// Dgraph's `needsListTypeRebuild`: whether a scalar becomes a list, whose values are then re-identified.
    ///
    /// # Errors
    ///
    /// `Type can't be changed from list to scalar for attr: [<attr>] without dropping it first.` for a list
    /// becoming a scalar.
    pub fn needs_list_type_rebuild(&self) -> Result<bool, ApplyError> {
        let Some(old) = &self.old_schema else {
            return Ok(false);
        };
        if self.current_schema.list && !old.list {
            return Ok(true);
        }
        if old.list && !self.current_schema.list {
            return Err(ApplyError::ListToScalarEmpty(attr_name(
                &self.current_schema.predicate,
            )));
        }
        Ok(false)
    }
}

/// The names of `a` not in `b`, in `a`'s order: Dgraph's `x.Diff` without its maps.
fn diff(a: &[String], b: &[String]) -> Vec<String> {
    a.iter().filter(|t| !b.contains(t)).cloned().collect()
}

/// The vector indexes of `current` not in `old`, and of `old` not in `current`, compared by name and options.
fn spec_diff(
    current: &[VectorIndexSpec],
    old: &[VectorIndexSpec],
) -> Result<(Vec<VectorIndexSpec>, Vec<VectorIndexSpec>), ApplyError> {
    let named = |specs: &[VectorIndexSpec]| -> Result<Vec<String>, ApplyError> {
        specs
            .iter()
            .map(|s| Ok(get_factory_create_spec_from_spec(s)?.name()))
            .collect()
    };
    let (current_names, old_names) = (named(current)?, named(old)?);
    let added = current
        .iter()
        .zip(&current_names)
        .filter(|(_, n)| !old_names.contains(n))
        .map(|(s, _)| s.clone())
        .collect();
    let deleted = old
        .iter()
        .zip(&old_names)
        .filter(|(_, n)| !current_names.contains(n))
        .map(|(s, _)| s.clone())
        .collect();
    Ok((added, deleted))
}
