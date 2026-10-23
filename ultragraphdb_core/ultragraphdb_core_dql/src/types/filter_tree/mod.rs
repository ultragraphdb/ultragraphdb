/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod filter_tree_clone;
mod filter_tree_debug;
mod filter_tree_debug_string;
mod filter_tree_drop;
mod filter_tree_part_eq;

use crate::types::function::Function;
use crate::types::vars::Vars;

/// The parsed `@filter` directive: an operator (`and`, `or`, `not`) with children at inner nodes, a function at
/// the leaves.
#[derive(Eq, Default)]
pub struct FilterTree {
    pub(crate) op: String,
    pub(crate) child: Vec<FilterTree>,
    pub(crate) func: Option<Function>,
}

impl FilterTree {
    pub(crate) fn with_op(op: &str) -> Self {
        let mut tree = Self::default();
        tree.op = op.to_string();
        tree
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

    /// Collects the variables the tree's functions need, each node before its children; counted without
    /// recursion.
    pub(crate) fn collect_vars(&self, v: &mut Vars) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if let Some(f) = &node.func {
                v.needs.extend(f.needs_var.iter().map(|va| va.name.clone()));
            }
            stack.extend(node.child.iter().rev());
        }
    }

    /// Whether any of the tree's functions needs a variable; checked without recursion.
    pub(crate) fn has_vars(&self) -> bool {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.func.as_ref().is_some_and(|f| !f.needs_var.is_empty()) {
                return true;
            }
            stack.extend(node.child.iter());
        }
        false
    }
}
