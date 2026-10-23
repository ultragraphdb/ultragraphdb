/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod filter_tree_debug_string;

use crate::types::function::Function;
use crate::types::vars::Vars;

/// The parsed `@filter` directive: an operator (`and`, `or`, `not`) with children at inner nodes, a function at
/// the leaves.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterTree {
    pub(crate) op: String,
    pub(crate) child: Vec<FilterTree>,
    pub(crate) func: Option<Function>,
}

impl FilterTree {
    pub(crate) fn with_op(op: &str) -> Self {
        Self {
            op: op.to_string(),
            ..Self::default()
        }
    }

    pub fn op(&self) -> &str {
        &self.op
    }

    pub fn child(&self) -> &[FilterTree] {
        &self.child
    }

    pub fn func(&self) -> Option<&Function> {
        self.func.as_ref()
    }

    pub(crate) fn collect_vars(&self, v: &mut Vars) {
        if let Some(f) = &self.func {
            v.needs.extend(f.needs_var.iter().map(|va| va.name.clone()));
        }
        for fch in &self.child {
            fch.collect_vars(v);
        }
    }

    pub(crate) fn has_vars(&self) -> bool {
        self.func.as_ref().is_some_and(|f| !f.needs_var.is_empty())
            || self.child.iter().any(FilterTree::has_vars)
    }
}
