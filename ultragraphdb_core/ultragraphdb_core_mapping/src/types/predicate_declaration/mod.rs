/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;

use crate::types::cardinality::Cardinality;
use crate::types::predicate_kind::PredicateKind;
use crate::types::vector_index_spec::VectorIndexSpec;

/// A predicate as the catalog declares it: what its objects may be, how many a subject holds, the datatype its
/// literals must have, whether its values are unique, its indexes, and the entity types it is declared on. [`PredicateDeclaration::new`] is the default
/// an undeclared predicate behaves as: either kind, multi-valued, no constraint, values not unique, no index.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PredicateDeclaration {
    predicate: NamedNode,
    kind: PredicateKind,
    cardinality: Cardinality,
    datatype: Option<NamedNode>,
    unique: bool,
    tokenizers: Vec<String>,
    count_index: bool,
    vector_index: Option<VectorIndexSpec>,
    /// The entity types whose nodes the predicate is declared on, which the data dictionary reports it under.
    domains: Vec<NamedNode>,
}

impl PredicateDeclaration {
    pub fn new(predicate: NamedNode) -> Self {
        Self {
            predicate,
            kind: PredicateKind::default(),
            cardinality: Cardinality::default(),
            datatype: None,
            unique: false,
            tokenizers: Vec::new(),
            count_index: false,
            vector_index: None,
            domains: Vec::new(),
        }
    }

    pub fn with_kind(mut self, kind: PredicateKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_cardinality(mut self, cardinality: Cardinality) -> Self {
        self.cardinality = cardinality;
        self
    }

    pub fn with_datatype(mut self, datatype: NamedNode) -> Self {
        self.datatype = Some(datatype);
        self
    }

    pub fn with_unique(mut self, unique: bool) -> Self {
        self.unique = unique;
        self
    }

    /// The tokenizers by name, as the tokenizer registry names them: `exact`, `term`, `int`, `day` and the rest.
    pub fn with_tokenizers(mut self, tokenizers: Vec<String>) -> Self {
        self.tokenizers = tokenizers;
        self
    }

    pub fn with_count_index(mut self, count_index: bool) -> Self {
        self.count_index = count_index;
        self
    }

    pub fn with_vector_index(mut self, spec: VectorIndexSpec) -> Self {
        self.vector_index = Some(spec);
        self
    }

    pub fn without_vector_index(mut self) -> Self {
        self.vector_index = None;
        self
    }

    pub fn with_domains(mut self, domains: Vec<NamedNode>) -> Self {
        self.domains = domains;
        self
    }

    pub fn predicate(&self) -> &NamedNode {
        &self.predicate
    }

    pub fn kind(&self) -> PredicateKind {
        self.kind
    }

    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }

    pub fn datatype(&self) -> Option<&NamedNode> {
        self.datatype.as_ref()
    }

    pub fn unique(&self) -> bool {
        self.unique
    }

    pub fn tokenizers(&self) -> &[String] {
        &self.tokenizers
    }

    pub fn count_index(&self) -> bool {
        self.count_index
    }

    pub fn vector_index(&self) -> Option<&VectorIndexSpec> {
        self.vector_index.as_ref()
    }

    pub fn domains(&self) -> &[NamedNode] {
        &self.domains
    }
}
