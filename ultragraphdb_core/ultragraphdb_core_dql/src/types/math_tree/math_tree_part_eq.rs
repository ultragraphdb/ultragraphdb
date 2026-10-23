/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::MathTree;

impl PartialEq for MathTree {
    /// Compares the trees node by node from a stack rather than by recursion, so that comparing deep trees takes
    /// no more call stack than comparing one node (ledger 32).
    fn eq(&self, other: &Self) -> bool {
        let mut todo = vec![(self, other)];
        while let Some((a, b)) = todo.pop() {
            if !a.eq_node(b) {
                return false;
            }
            todo.extend(a.child.iter().zip(&b.child));
        }
        true
    }
}

impl MathTree {
    /// Whether the nodes are equal, their children aside but for their number. Every field is named, so that a
    /// field added to the type cannot be left out of the comparison.
    fn eq_node(&self, other: &Self) -> bool {
        let MathTree {
            fn_name: fn_name_a,
            var: var_a,
            const_val: const_val_a,
            val: val_a,
            child: child_a,
        } = self;
        let MathTree {
            fn_name: fn_name_b,
            var: var_b,
            const_val: const_val_b,
            val: val_b,
            child: child_b,
        } = other;
        fn_name_a == fn_name_b
            && var_a == var_b
            && const_val_a == const_val_b
            && val_a == val_b
            && child_a.len() == child_b.len()
    }
}
