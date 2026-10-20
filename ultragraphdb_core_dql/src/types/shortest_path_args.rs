/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::function::Function;

/// The `from` and `to` of a shortest path query: a uid (`from: 0x01`) or a uid function (`from: uid(p)`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShortestPathArgs {
    pub(crate) from: Option<Function>,
    pub(crate) to: Option<Function>,
}

impl ShortestPathArgs {
    pub fn from(&self) -> Option<&Function> {
        self.from.as_ref()
    }

    pub fn to(&self) -> Option<&Function> {
        self.to.as_ref()
    }
}
