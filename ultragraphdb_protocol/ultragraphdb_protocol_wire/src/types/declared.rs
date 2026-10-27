/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::declared_entity_type::DeclaredEntityType;
use crate::types::declared_predicate::DeclaredPredicate;
use crate::types::declared_relation::DeclaredRelation;

/// A catalog declaration a Write carries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Declared {
    Predicate(DeclaredPredicate),
    EntityType(DeclaredEntityType),
    Relation(DeclaredRelation),
}
