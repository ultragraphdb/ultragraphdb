/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ColumnType;

/// A prepared statement: its id on the connection that prepared it, its namespace, the schema id of the catalog it was
/// prepared against, and its parameter and result column types. Once its namespace's schema id is another, an
/// execution is refused as stale and the statement is prepared again; on another connection it is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Statement {
    id: u64,
    namespace: u64,
    schema: u64,
    parameter_types: Vec<ColumnType>,
    result_types: Vec<ColumnType>,
}

impl Statement {
    pub(crate) fn new(
        id: u64,
        namespace: u64,
        schema: u64,
        parameter_types: Vec<ColumnType>,
        result_types: Vec<ColumnType>,
    ) -> Self {
        Self {
            id,
            namespace,
            schema,
            parameter_types,
            result_types,
        }
    }

    /// The statement's id on the connection that prepared it.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The namespace it was prepared in.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The schema id of the catalog it was prepared against.
    pub fn schema(&self) -> u64 {
        self.schema
    }

    /// The column types of its parameters.
    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    /// The column types of its results.
    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }

    /// Whether the statement was prepared against `schema`, its namespace's schema id as a Describe answers it.
    pub fn is_current(&self, schema: u64) -> bool {
        self.schema == schema
    }
}
