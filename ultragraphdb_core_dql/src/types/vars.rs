/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The variables a query block defines and the variables it uses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vars {
    pub(crate) defines: Vec<String>,
    pub(crate) needs: Vec<String>,
}

impl Vars {
    pub fn defines(&self) -> &[String] {
        &self.defines
    }

    pub fn needs(&self) -> &[String] {
        &self.needs
    }
}
