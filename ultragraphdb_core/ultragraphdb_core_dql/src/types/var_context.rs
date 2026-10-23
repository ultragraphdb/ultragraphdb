/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::var_type::VarType;

/// A variable a query needs to complete, with its kind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VarContext {
    pub(crate) name: String,
    pub(crate) typ: VarType,
}

impl VarContext {
    pub(crate) fn new(name: impl Into<String>, typ: VarType) -> Self {
        Self {
            name: name.into(),
            typ,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn typ(&self) -> VarType {
        self.typ
    }
}
