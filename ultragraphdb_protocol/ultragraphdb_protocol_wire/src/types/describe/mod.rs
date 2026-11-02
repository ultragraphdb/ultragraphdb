/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod describe_body_builder;

use crate::traits::body_builder::BodyBuilder;
use crate::types::declared_entity_type::DeclaredEntityType;
use crate::types::declared_predicate::DeclaredPredicate;
use crate::types::declared_relation::DeclaredRelation;
use crate::utils::checksum::xxh3_64;

/// What a namespace holds, the data dictionary's answer: sent with the namespace alone, answered with the snapshot it
/// was read at, the labels, relationship types and property keys, and the catalog's declarations: predicates, entity
/// types with their preorder numbers, and relations.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `read_version` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | strings: ends |
/// | 1 | strings: text, each IRI plain |
/// | 2 | labels, u32 string indices |
/// | 3 | relationship types, u32 string indices |
/// | 4 | property keys, u32 string indices |
/// | 5 | declarations: change records of kinds 8, 9 and 10, an entity type's preorder number in `extra` |
/// | 6 | the declarations' args |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Describe {
    namespace: u64,
    read_version: u64,
    labels: Vec<String>,
    relationship_types: Vec<String>,
    property_keys: Vec<String>,
    predicates: Vec<DeclaredPredicate>,
    entity_types: Vec<(DeclaredEntityType, u32)>,
    relations: Vec<DeclaredRelation>,
}

impl Describe {
    /// An answer for `namespace` read at `read_version`, empty until its parts are set.
    pub fn new(namespace: u64, read_version: u64) -> Self {
        Self {
            namespace,
            read_version,
            labels: Vec::new(),
            relationship_types: Vec::new(),
            property_keys: Vec::new(),
            predicates: Vec::new(),
            entity_types: Vec::new(),
            relations: Vec::new(),
        }
    }

    /// The namespace described.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The snapshot the answer was read at; zero in a request.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The labels.
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// The relationship types.
    pub fn relationship_types(&self) -> &[String] {
        &self.relationship_types
    }

    /// The property keys.
    pub fn property_keys(&self) -> &[String] {
        &self.property_keys
    }

    /// The declared predicates.
    pub fn predicates(&self) -> &[DeclaredPredicate] {
        &self.predicates
    }

    /// The declared entity types, each with its preorder number.
    pub fn entity_types(&self) -> &[(DeclaredEntityType, u32)] {
        &self.entity_types
    }

    /// The declared relations.
    pub fn relations(&self) -> &[DeclaredRelation] {
        &self.relations
    }

    /// The answer with its labels.
    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }

    /// The answer with its relationship types.
    pub fn with_relationship_types(mut self, relationship_types: Vec<String>) -> Self {
        self.relationship_types = relationship_types;
        self
    }

    /// The answer with its property keys.
    pub fn with_property_keys(mut self, property_keys: Vec<String>) -> Self {
        self.property_keys = property_keys;
        self
    }

    /// The answer with its declared predicates.
    pub fn with_predicates(mut self, predicates: Vec<DeclaredPredicate>) -> Self {
        self.predicates = predicates;
        self
    }

    /// The entity types, each with its preorder number.
    pub fn with_entity_types(mut self, entity_types: Vec<(DeclaredEntityType, u32)>) -> Self {
        self.entity_types = entity_types;
        self
    }

    /// The answer with its declared relations.
    pub fn with_relations(mut self, relations: Vec<DeclaredRelation>) -> Self {
        self.relations = relations;
        self
    }

    /// A request for `namespace`'s description.
    pub fn request(namespace: u64) -> Self {
        Self::new(namespace, 0)
    }

    /// The schema id of the declarations: XXH3-64 of the body of this answer with its namespace, labels, relationship
    /// types and property keys left out, so it changes with the catalog and not with the data. A prepared
    /// statement's schema id.
    pub fn schema_id(&self) -> u64 {
        let declarations = Describe::new(0, 0)
            .with_predicates(self.predicates.clone())
            .with_entity_types(self.entity_types.clone())
            .with_relations(self.relations.clone());
        xxh3_64(&declarations.body())
    }
}
