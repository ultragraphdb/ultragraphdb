/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/math.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod math_tree_debug_string;

use ultragraphdb_core_types::{ShardedMap, TypeId, Val};

use crate::types::vars::Vars;

/// A math expression in tree form: an operator or function with children at inner nodes; a variable or a
/// constant at the leaves.
#[derive(Debug, Clone, PartialEq)]
pub struct MathTree {
    pub(crate) fn_name: String,
    pub(crate) var: String,
    pub(crate) const_val: Val,
    pub(crate) val: Option<ShardedMap>,
    pub(crate) child: Vec<MathTree>,
}

impl Default for MathTree {
    fn default() -> Self {
        Self {
            fn_name: String::new(),
            var: String::new(),
            const_val: Val::nil(TypeId::DEFAULT),
            val: None,
            child: Vec::new(),
        }
    }
}

impl MathTree {
    pub(crate) fn with_fn(fn_name: &str) -> Self {
        Self {
            fn_name: fn_name.to_string(),
            ..Self::default()
        }
    }

    /// The operator or function, Go's `Fn`: `+`, `u-`, `exp`, `cond`, ...
    pub fn fn_name(&self) -> &str {
        &self.fn_name
    }

    /// The variable at a leaf.
    pub fn var(&self) -> &str {
        &self.var
    }

    /// The constant at a leaf, Go's `Const`: an int or a float as parsed, or a query parameter's typed value.
    /// Its value is `None` where Go's is nil.
    pub fn const_val(&self) -> &Val {
        &self.const_val
    }

    /// The values by uid, Go's `Val`, which query evaluation fills; `None` from the parser.
    pub fn val(&self) -> Option<&ShardedMap> {
        self.val.as_ref()
    }

    pub fn child(&self) -> &[MathTree] {
        &self.child
    }

    pub(crate) fn collect_vars(&self, v: &mut Vars) {
        if !self.var.is_empty() {
            v.needs.push(self.var.clone());
            return;
        }
        for fch in &self.child {
            fch.collect_vars(v);
        }
    }
}
