/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

pub(crate) mod catalog_checks;
mod catalog_store;
mod catalog_types;

use std::borrow::Cow;
use std::collections::BTreeMap;
use ultragraphdb_core_ids::SystemId;

use oxrdf::NamedNode;
use oxrdf::vocab::rdf;
use ultragraphdb_core_mapping::{
    Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_core_tok::{get_index_factory, get_tokenizer};

use crate::errors::catalog_error::CatalogError;
use crate::utils::catalog_codec::check_lengths;
use crate::utils::tokenizer_datatypes::tokenizer_takes;

/// The declarations of one tenant, by the ID of the predicate, entity type or relation each declares: what both
/// engines, index maintenance and the write checks read. A predicate it does not declare behaves as
/// [`PredicateDeclaration::new`], kind either, multi-valued, no datatype constraint, values not unique, no index.
/// A write works on a copy and the copy replaces the catalog when the write commits. A predicate declared here whose ID the
/// partition's dictionary does not hold, its bucket another partition's, is held by its IRI until a write resolves it: the
/// partition holds none of its data, and what it writes of it is written under the declaration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalog {
    tenant: u64,
    version: u64,
    predicates: BTreeMap<SystemId, PredicateDeclaration>,
    unresolved: BTreeMap<String, PredicateDeclaration>,
    types: BTreeMap<SystemId, TypeDeclaration>,
    relations: BTreeMap<SystemId, RelationDeclaration>,
}

impl Catalog {
    /// An empty catalog of `tenant`.
    pub fn new(tenant: u64) -> Self {
        Self {
            tenant,
            ..Self::default()
        }
    }

    /// A tenant's initial catalog: `rdf:type`, whose ID is `rdf_type`, declared an edge, and nothing else.
    pub fn initial(tenant: u64, rdf_type: SystemId) -> Self {
        let mut catalog = Self::new(tenant);
        let _none: Option<PredicateDeclaration> = catalog.predicates.insert(
            rdf_type,
            PredicateDeclaration::new(rdf::TYPE.into_owned()).with_kind(PredicateKind::Edge),
        );
        catalog
    }

    /// The catalog with `rdf:type`, whose ID is `rdf_type`, declared an edge, as [`Catalog::initial`] gives it, keeping what it
    /// holds by name.
    #[must_use]
    pub fn with_initial(mut self, rdf_type: SystemId) -> Self {
        let _none: Option<PredicateDeclaration> = self.predicates.insert(
            rdf_type,
            PredicateDeclaration::new(rdf::TYPE.into_owned()).with_kind(PredicateKind::Edge),
        );
        self
    }

    /// The catalog with nothing declared by ID, keeping its version and what it holds by name.
    #[must_use]
    pub fn emptied(self) -> Self {
        Self {
            tenant: self.tenant,
            version: self.version,
            unresolved: self.unresolved,
            ..Self::default()
        }
    }

    /// The tenant the catalog is of.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The number of the catalog version the catalog holds, 0 for one no version installed.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The catalog holding version `version`, its declarations unchanged.
    #[must_use]
    pub fn with_version(self, version: u64) -> Self {
        Self { version, ..self }
    }

    /// The declaration of predicate `uid`, if the catalog holds one.
    #[must_use]
    pub fn predicate(&self, uid: SystemId) -> Option<&PredicateDeclaration> {
        self.predicates.get(&uid)
    }

    /// The declaration predicate `uid`, named `iri`, behaves as: its own, or the default.
    #[must_use]
    pub fn predicate_or_default(
        &self,
        uid: SystemId,
        iri: &NamedNode,
    ) -> Cow<'_, PredicateDeclaration> {
        match self
            .predicates
            .get(&uid)
            .or_else(|| self.unresolved.get(iri.as_str()))
        {
            Some(declared) => Cow::Borrowed(declared),
            None => Cow::Owned(PredicateDeclaration::new(iri.clone())),
        }
    }

    /// The declaration of the predicate named `iri` whose ID no dictionary here held when it was declared, if there is one.
    #[must_use]
    pub fn unresolved(&self, iri: &str) -> Option<&PredicateDeclaration> {
        self.unresolved.get(iri)
    }

    /// Holds `declaration` by its IRI, its ID another partition's to give.
    ///
    /// # Errors
    ///
    /// The error [`Catalog::check_predicate`] gives `declaration`, the catalog unchanged.
    pub fn hold_unresolved(
        &mut self,
        declaration: PredicateDeclaration,
    ) -> Result<(), CatalogError> {
        Self::check_predicate(&declaration)?;
        let iri = declaration.predicate().as_str().to_string();
        let _replaced: Option<PredicateDeclaration> = self.unresolved.insert(iri, declaration);
        Ok(())
    }

    /// Forgets what [`Catalog::hold_unresolved`] held: a new version says it again.
    pub fn clear_unresolved(&mut self) {
        self.unresolved.clear();
    }

    /// Every predicate declared, by ID.
    pub fn predicates(&self) -> impl Iterator<Item = (SystemId, &PredicateDeclaration)> {
        self.predicates.iter().map(|(uid, d)| (*uid, d))
    }

    /// The declaration of entity type `uid`, if the catalog holds one.
    #[must_use]
    pub fn entity_type(&self, uid: SystemId) -> Option<&TypeDeclaration> {
        self.types.get(&uid)
    }

    /// Every entity type declared, by ID.
    pub fn entity_types(&self) -> impl Iterator<Item = (SystemId, &TypeDeclaration)> {
        self.types.iter().map(|(uid, d)| (*uid, d))
    }

    /// The declaration of relation `uid`, if the catalog holds one.
    #[must_use]
    pub fn relation(&self, uid: SystemId) -> Option<&RelationDeclaration> {
        self.relations.get(&uid)
    }

    /// Every relation declared, by ID.
    pub fn relations(&self) -> impl Iterator<Item = (SystemId, &RelationDeclaration)> {
        self.relations.iter().map(|(uid, d)| (*uid, d))
    }

    /// Declares predicate `uid`, replacing its declaration. Refused, changing nothing, as
    /// [`Catalog::check_predicate`] refuses.
    pub fn declare_predicate(
        &mut self,
        uid: SystemId,
        declaration: PredicateDeclaration,
    ) -> Result<(), CatalogError> {
        Self::check_predicate(&declaration)?;
        let _replaced: Option<PredicateDeclaration> = self.predicates.insert(uid, declaration);
        debug_assert!(self.predicates.contains_key(&uid));
        Ok(())
    }

    /// Whether `declaration` can be declared: refused when a tokenizer or vector index is unknown, or the datatype
    /// constraint is not among the datatypes one of its tokenizers takes.
    pub fn check_predicate(declaration: &PredicateDeclaration) -> Result<(), CatalogError> {
        let iri = declaration.predicate().as_str();
        check_lengths(&Declaration::Predicate(declaration.clone()))?;
        for tokenizer in declaration.tokenizers() {
            if get_tokenizer(tokenizer).is_none() {
                return Err(CatalogError::unknown_tokenizer(iri, tokenizer));
            }
            if let Some(datatype) = declaration.datatype()
                && !tokenizer_takes(tokenizer, datatype.as_str())
            {
                return Err(CatalogError::tokenizer_datatype(
                    iri,
                    tokenizer,
                    datatype.as_str(),
                ));
            }
        }
        if let Some(spec) = declaration.vector_index()
            && get_index_factory(spec.factory()).is_none()
        {
            return Err(CatalogError::unknown_vector_index(iri, spec.factory()));
        }
        Ok(())
    }

    /// Removes predicate `uid`'s declaration: it behaves as the default again. Whether it had one.
    pub fn remove_predicate(&mut self, uid: SystemId) -> bool {
        self.predicates.remove(&uid).is_some()
    }

    /// Removes entity type `uid`'s declaration: a type naming it as its parent becomes a root. Whether it had one.
    pub fn remove_entity_type(&mut self, uid: SystemId) -> bool {
        self.types.remove(&uid).is_some()
    }

    /// Removes relation `uid`'s declaration. Whether it had one.
    pub fn remove_relation(&mut self, uid: SystemId) -> bool {
        self.relations.remove(&uid).is_some()
    }

    /// Declares relation `uid`, replacing its declaration.
    ///
    /// # Errors
    ///
    /// [`CatalogError::too_large`] when its IRI or roles are longer than the catalog's encoding holds, the catalog
    /// unchanged.
    pub fn declare_relation(
        &mut self,
        uid: SystemId,
        declaration: RelationDeclaration,
    ) -> Result<(), CatalogError> {
        let declaration = Declaration::Relation(declaration);
        check_lengths(&declaration)?;
        if let Declaration::Relation(relation) = declaration {
            let _replaced: Option<RelationDeclaration> = self.relations.insert(uid, relation);
        }
        Ok(())
    }

    /// Applies `declaration` of the predicate, type or relation `uid` names; a type's parent, when it has one, is
    /// `parent`'s ID. Refused, changing nothing, as [`Catalog::declare_predicate`] and [`Catalog::declare_type`]
    /// refuse, and with [`CatalogError::too_large`] when a string or list is longer than the catalog's encoding holds.
    pub fn declare(
        &mut self,
        uid: SystemId,
        declaration: Declaration,
        parent: Option<SystemId>,
    ) -> Result<(), CatalogError> {
        // A predicate's lengths are checked by `check_predicate` and a relation's by `declare_relation`; a type's
        // here, so that every declaration taken encodes without loss.
        if matches!(declaration, Declaration::EntityType(_)) {
            check_lengths(&declaration)?;
        }
        match declaration {
            Declaration::Predicate(p) => self.declare_predicate(uid, p),
            Declaration::EntityType(t) => self.declare_type(uid, t, parent),
            Declaration::Relation(r) => self.declare_relation(uid, r),
        }
    }
}
