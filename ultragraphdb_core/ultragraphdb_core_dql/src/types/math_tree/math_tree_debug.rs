/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::MathTree;
use crate::utils::tree_debug::{Field, value, write_tree};

impl fmt::Debug for MathTree {
    /// The layout `#[derive(Debug)]` gives, written from a stack rather than by recursion, so that formatting a
    /// deep tree takes no more call stack than formatting one node (ledger 32).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_tree(f, self, "MathTree", |node, pretty, level| {
            // Every field is named, so that a field added to the type cannot be left out.
            let MathTree {
                fn_name,
                var,
                const_val,
                val,
                child,
            } = node;
            vec![
                Field::Value("fn_name", value(fn_name, pretty, level)),
                Field::Value("var", value(var, pretty, level)),
                Field::Value("const_val", value(const_val, pretty, level)),
                Field::Value("val", value(val, pretty, level)),
                Field::Children("child", child),
            ]
        })
    }
}
