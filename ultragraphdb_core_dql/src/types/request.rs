/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;

/// A query and the values of its parameters, keyed by name with the `$`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    pub(crate) str: String,
    pub(crate) variables: HashMap<String, String>,
}

impl Request {
    /// A query without parameters.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            str: query.into(),
            variables: HashMap::new(),
        }
    }

    /// A query with the values of its parameters, e.g. `{"$a": "3"}`.
    pub fn with_variables(query: impl Into<String>, variables: HashMap<String, String>) -> Self {
        Self {
            str: query.into(),
            variables,
        }
    }

    /// The query text.
    pub fn str(&self) -> &str {
        &self.str
    }

    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }
}
