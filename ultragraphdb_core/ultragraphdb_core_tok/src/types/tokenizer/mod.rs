/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok.go (the Tokenizer interface and its implementations) and
 * tok/index_factory.go (the vector index factory as a tokenizer). Changed for ultragraphdb: translated to Rust;
 * Go's interface over one struct per tokenizer as one type over a closed set, which static dispatch needs and
 * which leaves out Go's custom tokenizer plugins.
 * Copyright (c) 2026. Marvin Hansen.
 */

pub(crate) mod tokenizer_tokens;

use ultragraphdb_core_text::Collator;
use ultragraphdb_core_types::TypeId;

use crate::types::vector_index_factory::VectorIndexFactory;
use crate::utils::ident::{
    IDENT_BIG_FLOAT, IDENT_BOOL, IDENT_DAY, IDENT_DELIMITER, IDENT_EXACT, IDENT_EXACT_LANG,
    IDENT_FLOAT, IDENT_FULL_TEXT, IDENT_GEO, IDENT_HASH, IDENT_HOUR, IDENT_INT, IDENT_MONTH,
    IDENT_NGRAM, IDENT_SHA, IDENT_TERM, IDENT_TRIGRAM, IDENT_VFLOAT, IDENT_YEAR,
};

/// One of Dgraph's index tokenizers: it turns a value into index tokens, and says whether its tokens sort like the
/// values (`is_sortable`) and whether a token match has to be checked against the value (`is_lossy`).
///
/// Tokenizers are looked up by name with [`get_tokenizer`](crate::get_tokenizer); the term, fulltext and exact
/// tokenizers take a language with [`get_tokenizer_for_lang`](crate::get_tokenizer_for_lang).
#[derive(Debug, Clone)]
pub struct Tokenizer(Kind);

#[derive(Debug, Clone)]
pub(crate) enum Kind {
    BigFloat,
    Geo,
    Int,
    Float,
    Year,
    Month,
    Day,
    Hour,
    Term(String),
    Exact(Option<Box<ExactLang>>),
    Bool,
    Trigram,
    Hash,
    FullText(String),
    NGram(String),
    Sha256,
    Factory(VectorIndexFactory),
}

/// The language of an exact tokenizer with a language: its base and its collator.
#[derive(Debug, Clone)]
pub(crate) struct ExactLang {
    pub(crate) lang_base: String,
    pub(crate) collator: Collator,
}

impl Tokenizer {
    pub(crate) fn new(kind: Kind) -> Self {
        Self(kind)
    }

    pub(crate) fn kind(&self) -> &Kind {
        &self.0
    }

    /// Go's `Name`: the unique name the schema's `@index` names it by.
    pub fn name(&self) -> &'static str {
        match &self.0 {
            Kind::BigFloat => "bigfloat",
            Kind::Geo => "geo",
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::Year => "year",
            Kind::Month => "month",
            Kind::Day => "day",
            Kind::Hour => "hour",
            Kind::Term(_) => "term",
            Kind::Exact(_) => "exact",
            Kind::Bool => "bool",
            Kind::Trigram => "trigram",
            Kind::Hash => "hash",
            Kind::FullText(_) => "fulltext",
            Kind::NGram(_) => "ngram",
            Kind::Sha256 => "sha256",
            Kind::Factory(f) => f.name(),
        }
    }

    /// Go's `Type`: the name of the value type it tokenizes.
    pub fn type_name(&self) -> &'static str {
        match &self.0 {
            Kind::BigFloat => "bigfloat",
            Kind::Geo => "geo",
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::Year | Kind::Month | Kind::Day | Kind::Hour => "datetime",
            Kind::Bool => "bool",
            Kind::Factory(_) => "float32vector",
            _ => "string",
        }
    }

    /// The value type it tokenizes, `TypeForName` of [`type_name`](Self::type_name).
    pub fn type_id(&self) -> TypeId {
        match &self.0 {
            Kind::BigFloat => TypeId::BIGFLOAT,
            Kind::Geo => TypeId::GEO,
            Kind::Int => TypeId::INT,
            Kind::Float => TypeId::FLOAT,
            Kind::Year | Kind::Month | Kind::Day | Kind::Hour => TypeId::DATETIME,
            Kind::Bool => TypeId::BOOL,
            Kind::Factory(_) => TypeId::VFLOAT,
            _ => TypeId::STRING,
        }
    }

    /// Whether it tokenizes values of type `typ`, the check Dgraph's schema parser makes for `@index`.
    pub fn is_valid_for(&self, typ: TypeId) -> bool {
        self.type_id() == typ
    }

    /// Go's `Identifier`: the byte every token of this tokenizer starts with.
    pub fn identifier(&self) -> u8 {
        match &self.0 {
            Kind::BigFloat => IDENT_BIG_FLOAT,
            Kind::Geo => IDENT_GEO,
            Kind::Int => IDENT_INT,
            Kind::Float => IDENT_FLOAT,
            Kind::Year => IDENT_YEAR,
            Kind::Month => IDENT_MONTH,
            Kind::Day => IDENT_DAY,
            Kind::Hour => IDENT_HOUR,
            Kind::Term(_) => IDENT_TERM,
            Kind::Exact(None) => IDENT_EXACT,
            Kind::Exact(Some(_)) => IDENT_EXACT_LANG,
            Kind::Bool => IDENT_BOOL,
            Kind::Trigram => IDENT_TRIGRAM,
            Kind::Hash => IDENT_HASH,
            Kind::FullText(_) => IDENT_FULL_TEXT,
            Kind::NGram(_) => IDENT_NGRAM,
            Kind::Sha256 => IDENT_SHA,
            Kind::Factory(_) => IDENT_VFLOAT,
        }
    }

    /// Go's `IsSortable`: whether its tokens' byte order is the values' order.
    pub fn is_sortable(&self) -> bool {
        matches!(
            self.0,
            Kind::BigFloat
                | Kind::Int
                | Kind::Float
                | Kind::Year
                | Kind::Month
                | Kind::Day
                | Kind::Hour
                | Kind::Exact(_)
        )
    }

    /// Go's `IsLossy`: whether a value matching a token has to be fetched and compared, because the token does not
    /// hold the value.
    pub fn is_lossy(&self) -> bool {
        !matches!(
            self.0,
            Kind::Int | Kind::Exact(_) | Kind::Bool | Kind::Hash | Kind::Sha256
        )
    }

    /// The exact tokenizer's `Prefix`: the identifier, and with a language its base and the delimiter `0x1F`, which
    /// every token starts with. `None` for every other tokenizer.
    pub fn prefix(&self) -> Option<Vec<u8>> {
        match &self.0 {
            Kind::Exact(None) => Some(vec![IDENT_EXACT]),
            Kind::Exact(Some(l)) => {
                let mut p = vec![IDENT_EXACT_LANG];
                p.extend_from_slice(l.lang_base.as_bytes());
                p.push(IDENT_DELIMITER);
                Some(p)
            }
            _ => None,
        }
    }

    /// The vector index factory this tokenizer is, for `hnsw`.
    pub fn index_factory(&self) -> Option<&VectorIndexFactory> {
        match &self.0 {
            Kind::Factory(f) => Some(f),
            _ => None,
        }
    }
}
