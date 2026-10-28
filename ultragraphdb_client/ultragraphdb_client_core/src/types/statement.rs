/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ColumnType;

/// A prepared statement: its id in the session, its namespace, the schema id of the catalog it was prepared against,
/// and its parameter and result column types. Once its namespace's schema id is another, an execution is refused as
/// stale and the statement is prepared again.
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

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn schema(&self) -> u64 {
        self.schema
    }

    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }

    /// Whether the statement was prepared against `schema`, its namespace's schema id as a Describe answers it.
    pub fn is_current(&self, schema: u64) -> bool {
        self.schema == schema
    }
}
