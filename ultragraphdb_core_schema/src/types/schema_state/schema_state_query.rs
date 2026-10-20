/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashSet;

use ultragraphdb_core_keys::parse_namespace;
use ultragraphdb_core_tok::{
    FactoryCreateSpec, Tokenizer, VEC_DEAD, VEC_ENTRY, VEC_KEYWORD,
    get_factory_create_spec_from_spec, get_tokenizer,
};
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

use super::SchemaState;
use crate::errors::schema_error::SchemaError;
use crate::types::schema_mode::SchemaMode;

impl SchemaState {
    /// Dgraph's `Namespaces`: the namespaces the types are in.
    pub fn namespaces(&self) -> HashSet<u64> {
        self.read()
            .types
            .keys()
            .map(|typ| parse_namespace(typ))
            .collect()
    }

    /// Dgraph's `GetIndexingPredicates`: the predicates whose indexes are being built, in no order.
    pub fn indexing_predicates(&self) -> Vec<String> {
        self.read().mut_schema.keys().cloned().collect()
    }

    /// Dgraph's `IndexingInProgress`: whether any index is being built.
    pub fn indexing_in_progress(&self) -> bool {
        !self.read().mut_schema.is_empty()
    }

    /// Dgraph's `Get`: the full schema of a predicate, `unique` included; in write mode the background one where
    /// there is one.
    pub fn get(&self, mode: SchemaMode, pred: &str) -> Option<pb::SchemaUpdate> {
        self.read().schema_for(mode, pred).cloned()
    }

    /// Dgraph's `GetType`: the definition of a type.
    pub fn get_type(&self, type_name: &str) -> Option<pb::TypeUpdate> {
        self.read().types.get(type_name).cloned()
    }

    /// Dgraph's `TypeOf`: the value type of a predicate.
    ///
    /// # Errors
    ///
    /// `Schema not defined for predicate: <pred>.` when the state has no schema for it.
    pub fn type_of(&self, pred: &str) -> Result<TypeId, SchemaError> {
        self.read()
            .predicate
            .get(pred)
            .map(|schema| TypeId::new(schema.value_type))
            .ok_or_else(|| SchemaError::state(format!("Schema not defined for predicate: {pred}.")))
    }

    /// Dgraph's `IsIndexed`: whether the predicate has a tokenizer or a vector index.
    pub fn is_indexed(&self, mode: SchemaMode, pred: &str) -> bool {
        self.read().holds(mode, pred, |schema| {
            !schema.tokenizer.is_empty() || !schema.index_specs.is_empty()
        })
    }

    /// Dgraph's `Predicates`: every predicate, in no order.
    pub fn predicates(&self) -> Vec<String> {
        self.read().predicate.keys().cloned().collect()
    }

    /// Dgraph's `Types`: every type, in no order.
    pub fn types(&self) -> Vec<String> {
        self.read().types.keys().cloned().collect()
    }

    /// Dgraph's `Tokenizer`: the tokenizers of a predicate, none when the state has no schema for it (it may have
    /// been dropped while a query was running).
    ///
    /// # Panics
    ///
    /// When the schema names a tokenizer that does not exist, as Dgraph's assertion does; parsing refuses one.
    pub fn tokenizer(&self, mode: SchemaMode, pred: &str) -> Vec<Tokenizer> {
        let s = self.read();
        let Some(schema) = s.schema_for(mode, pred) else {
            return Vec::new();
        };
        schema
            .tokenizer
            .iter()
            .map(|name| get_tokenizer(name).unwrap_or_else(|| panic!("Invalid tokenizer {name}")))
            .collect()
    }

    /// Dgraph's `FactoryCreateSpec`: the vector index factory and options of each vector index of a predicate.
    ///
    /// # Errors
    ///
    /// `Schema state not found for <pred>.` when the state has no schema for it, or the error of a specification
    /// the registry refuses.
    pub fn factory_create_spec(
        &self,
        mode: SchemaMode,
        pred: &str,
    ) -> Result<Vec<FactoryCreateSpec>, SchemaError> {
        let s = self.read();
        let Some(schema) = s.schema_for(mode, pred) else {
            return Err(SchemaError::state(format!(
                "Schema state not found for {pred}."
            )));
        };
        schema
            .index_specs
            .iter()
            .map(|spec| {
                get_factory_create_spec_from_spec(spec)
                    .map_err(|e| SchemaError::state(e.to_string()))
            })
            .collect()
    }

    /// Dgraph's `VectorIndexes`: the names of a predicate's vector indexes with their options, e.g.
    /// `hnsw("metric":"cosine")`; none when [`factory_create_spec`](Self::factory_create_spec) fails, which Dgraph
    /// logs.
    pub fn vector_indexes(&self, mode: SchemaMode, pred: &str) -> Vec<String> {
        self.factory_create_spec(mode, pred)
            .unwrap_or_default()
            .iter()
            .map(FactoryCreateSpec::name)
            .collect()
    }

    /// Dgraph's `TokenizerNames`: the names of a predicate's tokenizers.
    pub fn tokenizer_names(&self, mode: SchemaMode, pred: &str) -> Vec<String> {
        self.tokenizer(mode, pred)
            .iter()
            .map(|t| t.name().to_string())
            .collect()
    }

    /// Dgraph's `HasTokenizer`: whether a predicate has the tokenizer with identifier `id`.
    pub fn has_tokenizer(&self, mode: SchemaMode, id: u8, pred: &str) -> bool {
        self.tokenizer(mode, pred)
            .iter()
            .any(|t| t.identifier() == id)
    }

    /// Dgraph's `IsReversed`: whether a predicate has a reverse edge.
    pub fn is_reversed(&self, mode: SchemaMode, pred: &str) -> bool {
        self.read().holds(mode, pred, |schema| {
            schema.directive == Directive::Reverse as i32
        })
    }

    /// Dgraph's `HasCount`: whether a count index is kept for a predicate.
    pub fn has_count(&self, mode: SchemaMode, pred: &str) -> bool {
        self.read().holds(mode, pred, |schema| schema.count)
    }

    /// Dgraph's `PredicatesToDelete`: the predicate and, for an indexed vector predicate, the three predicates its
    /// vector index keeps beside it; none when the state has no schema for it.
    pub fn predicates_to_delete(&self, pred: &str) -> Vec<String> {
        let s = self.read();
        let Some(schema) = s.predicate.get(pred) else {
            return Vec::new();
        };
        if schema.value_type == pb::posting::ValType::Vfloat as i32
            && !schema.index_specs.is_empty()
        {
            return vec![
                pred.to_string(),
                format!("{pred}{VEC_ENTRY}"),
                format!("{pred}{VEC_KEYWORD}"),
                format!("{pred}{VEC_DEAD}"),
            ];
        }
        vec![pred.to_string()]
    }

    /// Dgraph's `IsList`: whether a predicate holds a list.
    pub fn is_list(&self, pred: &str) -> bool {
        self.read()
            .holds(SchemaMode::Query, pred, |schema| schema.list)
    }

    /// Dgraph's `HasUpsert`: whether a predicate has `@upsert`.
    pub fn has_upsert(&self, pred: &str) -> bool {
        self.read()
            .holds(SchemaMode::Query, pred, |schema| schema.upsert)
    }

    /// Dgraph's `HasLang`: whether a predicate has `@lang`.
    pub fn has_lang(&self, pred: &str) -> bool {
        self.read()
            .holds(SchemaMode::Query, pred, |schema| schema.lang)
    }

    /// Dgraph's `HasNoConflict`: whether a predicate has `@noconflict`.
    pub fn has_no_conflict(&self, pred: &str) -> bool {
        self.read()
            .holds(SchemaMode::Query, pred, |schema| schema.no_conflict)
    }
}
