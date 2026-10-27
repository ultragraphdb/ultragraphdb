/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::declared_kind::DeclaredKind;

/// A predicate's declaration as Write and Describe carry it, every IRI by its text: its kind, whether it is
/// single-valued and unique, its datatype, tokenizers, count index, domains, and vector index with its factory and
/// options.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DeclaredPredicate {
    predicate: String,
    kind: DeclaredKind,
    single: bool,
    unique: bool,
    datatype: Option<String>,
    tokenizers: Vec<String>,
    count_index: bool,
    domains: Vec<String>,
    vector_index: Option<(String, Vec<(String, String)>)>,
}

impl DeclaredPredicate {
    /// The declaration of `predicate`: of either kind, multi-valued, with nothing else declared.
    pub fn new(predicate: impl Into<String>) -> Self {
        Self {
            predicate: predicate.into(),
            ..Self::default()
        }
    }

    pub fn with_kind(self, kind: DeclaredKind) -> Self {
        Self { kind, ..self }
    }

    pub fn with_single(self, single: bool) -> Self {
        Self { single, ..self }
    }

    pub fn with_unique(self, unique: bool) -> Self {
        Self { unique, ..self }
    }

    pub fn with_datatype(self, datatype: impl Into<String>) -> Self {
        Self {
            datatype: Some(datatype.into()),
            ..self
        }
    }

    pub fn with_tokenizers(self, tokenizers: Vec<String>) -> Self {
        Self { tokenizers, ..self }
    }

    pub fn with_count_index(self, count_index: bool) -> Self {
        Self {
            count_index,
            ..self
        }
    }

    pub fn with_domains(self, domains: Vec<String>) -> Self {
        Self { domains, ..self }
    }

    /// The declaration with a vector index of `factory` and its `options` by name.
    pub fn with_vector_index(
        self,
        factory: impl Into<String>,
        options: Vec<(String, String)>,
    ) -> Self {
        Self {
            vector_index: Some((factory.into(), options)),
            ..self
        }
    }

    pub fn predicate(&self) -> &str {
        &self.predicate
    }

    pub fn kind(&self) -> DeclaredKind {
        self.kind
    }

    pub fn single(&self) -> bool {
        self.single
    }

    pub fn unique(&self) -> bool {
        self.unique
    }

    pub fn datatype(&self) -> Option<&str> {
        self.datatype.as_deref()
    }

    pub fn tokenizers(&self) -> &[String] {
        &self.tokenizers
    }

    pub fn count_index(&self) -> bool {
        self.count_index
    }

    pub fn domains(&self) -> &[String] {
        &self.domains
    }

    /// The vector index's factory and options.
    pub fn vector_index(&self) -> Option<&(String, Vec<(String, String)>)> {
        self.vector_index.as_ref()
    }
}
