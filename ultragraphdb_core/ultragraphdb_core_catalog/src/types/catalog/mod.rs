/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

pub(crate) mod catalog_checks;
mod catalog_store;
mod catalog_types;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use oxrdf::NamedNode;
use oxrdf::vocab::rdf;
use ultragraphdb_core_mapping::{
    Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_core_tok::{get_index_factory, get_tokenizer};

use crate::errors::catalog_error::CatalogError;
use crate::utils::tokenizer_datatypes::tokenizer_takes;

/// The declarations of one namespace, by the UID of the predicate, entity type or relation each declares: what both
/// engines, index maintenance and the write checks read. A predicate it does not declare behaves as
/// [`PredicateDeclaration::new`], kind either, multi-valued, no datatype constraint, values not unique, no index.
/// A write works on a copy and the copy replaces the catalog when the write commits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalog {
    namespace: u64,
    predicates: BTreeMap<u64, PredicateDeclaration>,
    types: BTreeMap<u64, TypeDeclaration>,
    relations: BTreeMap<u64, RelationDeclaration>,
    /// The predicates some statement of which has carried a statement ID.
    with_statements: BTreeSet<u64>,
}

impl Catalog {
    /// An empty catalog of `namespace`.
    pub fn new(namespace: u64) -> Self {
        Self {
            namespace,
            ..Self::default()
        }
    }

    /// A namespace's initial catalog: `rdf:type`, whose UID is `rdf_type`, declared an edge, and nothing else.
    pub fn initial(namespace: u64, rdf_type: u64) -> Self {
        let mut catalog = Self::new(namespace);
        catalog.predicates.insert(
            rdf_type,
            PredicateDeclaration::new(rdf::TYPE.into_owned()).with_kind(PredicateKind::Edge),
        );
        catalog
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The declaration of predicate `uid`, if the catalog holds one.
    pub fn predicate(&self, uid: u64) -> Option<&PredicateDeclaration> {
        self.predicates.get(&uid)
    }

    /// The declaration predicate `uid`, named `iri`, behaves as: its own, or the default.
    pub fn predicate_or_default(&self, uid: u64, iri: &NamedNode) -> Cow<'_, PredicateDeclaration> {
        match self.predicates.get(&uid) {
            Some(declared) => Cow::Borrowed(declared),
            None => Cow::Owned(PredicateDeclaration::new(iri.clone())),
        }
    }

    /// Every predicate declared, by UID.
    pub fn predicates(&self) -> impl Iterator<Item = (u64, &PredicateDeclaration)> {
        self.predicates.iter().map(|(uid, d)| (*uid, d))
    }

    pub fn entity_type(&self, uid: u64) -> Option<&TypeDeclaration> {
        self.types.get(&uid)
    }

    pub fn entity_types(&self) -> impl Iterator<Item = (u64, &TypeDeclaration)> {
        self.types.iter().map(|(uid, d)| (*uid, d))
    }

    pub fn relation(&self, uid: u64) -> Option<&RelationDeclaration> {
        self.relations.get(&uid)
    }

    pub fn relations(&self) -> impl Iterator<Item = (u64, &RelationDeclaration)> {
        self.relations.iter().map(|(uid, d)| (*uid, d))
    }

    /// Declares predicate `uid`, replacing its declaration. Refused, changing nothing, as
    /// [`Catalog::check_predicate`] refuses.
    pub fn declare_predicate(
        &mut self,
        uid: u64,
        declaration: PredicateDeclaration,
    ) -> Result<(), CatalogError> {
        Self::check_predicate(&declaration)?;
        self.predicates.insert(uid, declaration);
        Ok(())
    }

    /// Whether `declaration` can be declared: refused when a tokenizer or vector index is unknown, or the datatype
    /// constraint is not among the datatypes one of its tokenizers takes.
    pub fn check_predicate(declaration: &PredicateDeclaration) -> Result<(), CatalogError> {
        let iri = declaration.predicate().as_str();
        for tokenizer in declaration.tokenizers() {
            if get_tokenizer(tokenizer).is_none() {
                return Err(CatalogError::UnknownTokenizer(iri, tokenizer));
            }
            if let Some(datatype) = declaration.datatype()
                && !tokenizer_takes(tokenizer, datatype.as_str())
            {
                return Err(CatalogError::TokenizerDatatype(
                    iri,
                    tokenizer,
                    datatype.as_str(),
                ));
            }
        }
        if let Some(spec) = declaration.vector_index()
            && get_index_factory(spec.factory()).is_none()
        {
            return Err(CatalogError::UnknownVectorIndex(iri, spec.factory()));
        }
        Ok(())
    }

    /// Removes predicate `uid`'s declaration: it behaves as the default again. Whether it had one.
    pub fn remove_predicate(&mut self, uid: u64) -> bool {
        self.predicates.remove(&uid).is_some()
    }

    /// Whether a statement of predicate `uid` has carried a statement ID, so that an insert of it reads the
    /// subject's list to keep the IDs a held object carries.
    pub fn has_statements(&self, uid: u64) -> bool {
        self.with_statements.contains(&uid)
    }

    /// Marks predicate `uid` as carrying statement IDs: whether the mark is new.
    pub fn mark_statements(&mut self, uid: u64) -> bool {
        self.with_statements.insert(uid)
    }

    pub fn declare_relation(&mut self, uid: u64, declaration: RelationDeclaration) {
        self.relations.insert(uid, declaration);
    }

    /// Applies `declaration` of the predicate, type or relation `uid` names; a type's parent, when it has one, is
    /// `parent`'s UID.
    pub fn declare(
        &mut self,
        uid: u64,
        declaration: Declaration,
        parent: Option<u64>,
    ) -> Result<(), CatalogError> {
        match declaration {
            Declaration::Predicate(p) => self.declare_predicate(uid, p),
            Declaration::EntityType(t) => self.declare_type(uid, t, parent),
            Declaration::Relation(r) => {
                self.declare_relation(uid, r);
                Ok(())
            }
        }
    }
}
