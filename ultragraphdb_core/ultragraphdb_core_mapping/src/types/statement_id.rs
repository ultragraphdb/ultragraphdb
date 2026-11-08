/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{SystemId, TypePrefix};

use crate::errors::mapping_error::MappingError;

/// A statement's identity inside the store: a system ID of the built-in `Statement` type
/// ([`TypePrefix::STATEMENT`]) in its subject's bucket, given only to a statement that carries properties or repeats a
/// subject, predicate, object and graph already held. A client names the statement by its reifier, a skolem IRI the
/// dictionary maps to this ID, never by the ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatementId(SystemId);

impl StatementId {
    /// The statement ID `id`.
    ///
    /// # Errors
    ///
    /// [`MappingError::not_a_statement_id`] when `id` is not of the `Statement` type.
    pub fn new(id: SystemId) -> Result<Self, MappingError> {
        if id.type_prefix() == TypePrefix::STATEMENT {
            Ok(Self(id))
        } else {
            Err(MappingError::not_a_statement_id(id))
        }
    }

    /// The system ID.
    #[must_use]
    pub fn id(&self) -> SystemId {
        self.0
    }
}
