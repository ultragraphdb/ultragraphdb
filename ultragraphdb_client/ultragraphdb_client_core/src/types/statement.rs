/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ColumnType;

/// A prepared statement: its id on the connection that prepared it, its tenant, the schema id of the catalog it was
/// prepared against, and its parameter and result column types. Once its tenant's schema id is another, an
/// execution is refused as stale and the statement is prepared again; on another connection it is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Statement {
    id: u64,
    tenant: u64,
    schema: u64,
    parameter_types: Vec<ColumnType>,
    result_types: Vec<ColumnType>,
}

impl Statement {
    pub(crate) fn new(
        id: u64,
        tenant: u64,
        schema: u64,
        parameter_types: Vec<ColumnType>,
        result_types: Vec<ColumnType>,
    ) -> Self {
        Self {
            id,
            tenant,
            schema,
            parameter_types,
            result_types,
        }
    }

    /// The statement's id on the connection that prepared it.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The tenant it was prepared in.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The schema id of the catalog it was prepared against.
    #[must_use]
    pub fn schema(&self) -> u64 {
        self.schema
    }

    /// The column types of its parameters.
    #[must_use]
    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    /// The column types of its results.
    #[must_use]
    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }

    /// Whether the statement was prepared against `schema`, its tenant's schema id as a Describe answers it.
    #[must_use]
    pub fn is_current(&self, schema: u64) -> bool {
        self.schema == schema
    }
}
