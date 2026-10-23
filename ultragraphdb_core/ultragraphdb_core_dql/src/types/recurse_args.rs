/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

/// The arguments of the `@recurse` directive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecurseArgs {
    pub(crate) depth: u64,
    pub(crate) allow_loop: bool,
    /// The query parameters given for `depth` and `loop`, substituted once the request's variables are known.
    pub(crate) var_map: Option<BTreeMap<String, String>>,
}

impl RecurseArgs {
    pub fn depth(&self) -> u64 {
        self.depth
    }

    pub fn allow_loop(&self) -> bool {
        self.allow_loop
    }

    /// The query parameters named for `depth` and `loop`, `None` when neither took one.
    pub fn var_map(&self) -> Option<&BTreeMap<String, String>> {
        self.var_map.as_ref()
    }
}
