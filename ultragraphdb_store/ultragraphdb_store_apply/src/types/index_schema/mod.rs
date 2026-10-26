/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (indexTokens) and the schema state's write-context answers
 * (schema/schema.go: Get, TypeOf, IsIndexed, Tokenizer, FactoryCreateSpec, IsReversed, HasCount, IsList) index
 * maintenance asks. Changed for ultragraphdb: translated to Rust; the answers are read once per edge into a value
 * the maintenance takes, where Dgraph asks its global state at each step; while a predicate's indexes are rebuilt,
 * it holds the previous schema's indexes as well as the new schema's, each tokenizer tokenizing under the value type
 * of the schema that declares it, where Dgraph maintains the new schema's alone.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_catalog::{SchemaMode, SchemaState};
use ultragraphdb_core_tok::{
    FactoryCreateSpec, Tokenizer, build_tokens, get_factory_create_spec_from_spec, get_tokenizer,
    get_tokenizer_for_lang,
};
use ultragraphdb_core_types::{TypeId, Val, convert};
use ultragraphdb_proto_pb::pb::SchemaUpdate;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

use crate::errors::apply_error::ApplyError;

/// A tokenizer an edge is indexed by, with the value type its schema declares.
#[derive(Debug, Clone)]
struct IndexTokenizer {
    tokenizer: Tokenizer,
    value_type: TypeId,
    /// Declared by the previous schema alone, during a rebuild.
    previous_only: bool,
}

/// The schema index maintenance applies an edge of one predicate under: the predicate's write schema, its index
/// tokenizers and vector indexes, its reverse edges and count index, and whether it is a list.
#[derive(Debug, Clone, Default)]
pub struct IndexSchema {
    attr: String,
    schema: Option<SchemaUpdate>,
    value_type: Option<TypeId>,
    list: bool,
    tokenizers: Vec<IndexTokenizer>,
    specs: Vec<FactoryCreateSpec>,
    indexed: bool,
    reverse: bool,
    count: bool,
}

impl IndexSchema {
    /// The schema of `attr` as `state` answers a write: the schema being applied in the background where there is
    /// one, else the current one.
    ///
    /// # Errors
    ///
    /// A tokenizer or vector index the schema names and the registry does not hold.
    pub fn from_state(state: &SchemaState, attr: &str) -> Result<Self, ApplyError> {
        let schema = state.get(SchemaMode::Write, attr);
        let value_type = state.type_of(attr).ok();
        let tokenizers = state
            .tokenizer(SchemaMode::Write, attr)?
            .into_iter()
            .map(|tokenizer| IndexTokenizer {
                tokenizer,
                value_type: value_type.unwrap_or(TypeId::DEFAULT),
                previous_only: false,
            })
            .collect();
        let specs = if schema.is_some() {
            state.factory_create_spec(SchemaMode::Write, attr)?
        } else {
            Vec::new()
        };
        Ok(Self {
            attr: attr.to_string(),
            indexed: state.is_indexed(SchemaMode::Write, attr),
            reverse: state.is_reversed(SchemaMode::Write, attr),
            count: state.has_count(SchemaMode::Write, attr),
            list: state.is_list(attr),
            schema,
            value_type,
            tokenizers,
            specs,
        })
    }

    /// The schema `su` declares, alone: what a predicate a request creates is applied under.
    pub fn from_update(attr: &str, su: &SchemaUpdate) -> Result<Self, ApplyError> {
        let value_type = TypeId::new(su.value_type);
        Ok(Self {
            attr: attr.to_string(),
            schema: Some(su.clone()),
            value_type: Some(value_type),
            list: su.list,
            tokenizers: tokenizers_of(su, value_type, false)?,
            specs: specs_of(su)?,
            indexed: !su.tokenizer.is_empty() || !su.index_specs.is_empty(),
            reverse: su.directive == Directive::Reverse as i32,
            count: su.count,
        })
    }

    /// The schema an edge is applied under while the indexes of `new` are built over those of `old`: `new`'s, with
    /// the tokenizers, vector indexes, reverse edges and count index `old` declares beside it.
    pub fn during_rebuild(
        attr: &str,
        old: &SchemaUpdate,
        new: &SchemaUpdate,
    ) -> Result<Self, ApplyError> {
        let mut schema = Self::from_update(attr, new)?;
        let previous: Vec<IndexTokenizer> = tokenizers_of(old, TypeId::new(old.value_type), true)?
            .into_iter()
            .filter(|t| !new.tokenizer.iter().any(|n| n == t.tokenizer.name()))
            .collect();
        schema.tokenizers.extend(previous);
        let names: Vec<String> = schema.specs.iter().map(FactoryCreateSpec::name).collect();
        schema.specs.extend(
            specs_of(old)?
                .into_iter()
                .filter(|s| !names.contains(&s.name())),
        );
        schema.indexed = !schema.tokenizers.is_empty() || !schema.specs.is_empty();
        schema.reverse |= old.directive == Directive::Reverse as i32;
        schema.count |= old.count;
        Ok(schema)
    }

    pub fn attr(&self) -> &str {
        &self.attr
    }

    /// The write schema, `None` when the predicate has none.
    pub fn schema(&self) -> Option<&SchemaUpdate> {
        self.schema.as_ref()
    }

    pub fn is_list(&self) -> bool {
        self.list
    }

    pub fn is_indexed(&self) -> bool {
        self.indexed
    }

    pub fn is_reversed(&self) -> bool {
        self.reverse
    }

    pub fn has_count(&self) -> bool {
        self.count
    }

    /// The vector indexes, Dgraph's `FactoryCreateSpec`s.
    pub fn specs(&self) -> &[FactoryCreateSpec] {
        &self.specs
    }

    /// Whether the predicate has a tokenizer index.
    pub fn has_tokenizers(&self) -> bool {
        !self.tokenizers.is_empty()
    }

    /// The schema the posting lists of the predicate's keys are written with: its value type, list flag and count
    /// index.
    pub(crate) fn posting_schema(&self) -> Option<SchemaUpdate> {
        self.schema.as_ref().map(|s| SchemaUpdate {
            value_type: s.value_type,
            list: s.list,
            count: self.count,
            ..SchemaUpdate::default()
        })
    }

    /// Dgraph's `indexTokens`: the index tokens of `val`, without the predicate's prefix, each tokenizer tokenizing
    /// the value converted to its schema's type, in language `lang`. During a rebuild, a value the previous schema's
    /// type cannot hold has no tokens of the previous schema's tokenizers.
    ///
    /// # Errors
    ///
    /// `Cannot index attribute <attr> of type object.` for a predicate without a scalar type, `Attribute <attr> is not
    /// indexed.` for one without an index, and the conversion's or the tokenizer's error.
    pub fn tokens(&self, lang: &str, val: &Val) -> Result<Vec<Vec<u8>>, ApplyError> {
        if !self.value_type.is_some_and(TypeId::is_scalar) {
            return Err(ApplyError::NotIndexable(&self.attr));
        }
        if !self.indexed {
            return Err(ApplyError::NotIndexed(&self.attr));
        }
        let mut tokens = Vec::new();
        for t in &self.tokenizers {
            let sv = match convert(val, t.value_type) {
                Ok(sv) => sv,
                Err(error) if t.previous_only => {
                    tracing::warn!(attr = %self.attr, tokenizer = t.tokenizer.name(), %error,
                        "value not indexed by the previous schema's tokenizer during a rebuild");
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let Some(value) = sv.value() else {
                continue;
            };
            tokens.extend(build_tokens(
                value,
                &get_tokenizer_for_lang(&t.tokenizer, lang),
            )?);
        }
        Ok(tokens)
    }
}

fn tokenizers_of(
    su: &SchemaUpdate,
    value_type: TypeId,
    previous_only: bool,
) -> Result<Vec<IndexTokenizer>, ApplyError> {
    su.tokenizer
        .iter()
        .map(|name| {
            let tokenizer =
                get_tokenizer(name).ok_or_else(|| ApplyError::UnknownTokenizer(name))?;
            Ok(IndexTokenizer {
                tokenizer,
                value_type,
                previous_only,
            })
        })
        .collect()
}

fn specs_of(su: &SchemaUpdate) -> Result<Vec<FactoryCreateSpec>, ApplyError> {
    su.index_specs
        .iter()
        .map(|s| Ok(get_factory_create_spec_from_spec(s)?))
        .collect()
}
