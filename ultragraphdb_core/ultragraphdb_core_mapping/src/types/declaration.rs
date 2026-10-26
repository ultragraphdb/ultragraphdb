/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::predicate_declaration::PredicateDeclaration;
use crate::types::relation_declaration::RelationDeclaration;
use crate::types::type_declaration::TypeDeclaration;

/// A catalog declaration a changeset carries: what both engines and index maintenance read about a predicate, an
/// entity type or a relation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Declaration {
    Predicate(PredicateDeclaration),
    EntityType(TypeDeclaration),
    Relation(RelationDeclaration),
}
