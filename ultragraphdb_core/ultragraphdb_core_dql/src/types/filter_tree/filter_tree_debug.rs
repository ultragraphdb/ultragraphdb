/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::FilterTree;
use crate::utils::tree_debug::{Field, value, write_tree};

impl fmt::Debug for FilterTree {
    /// The layout `#[derive(Debug)]` gives, written from a stack rather than by recursion, so that formatting a
    /// deep tree takes no more call stack than formatting one node (ledger 32).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_tree(f, self, "FilterTree", |node, pretty, level| {
            // Every field is named, so that a field added to the type cannot be left out.
            let FilterTree { op, child, func } = node;
            vec![
                Field::Value("op", value(op, pretty, level)),
                Field::Children("child", child),
                Field::Value("func", value(func, pretty, level)),
            ]
        })
    }
}
