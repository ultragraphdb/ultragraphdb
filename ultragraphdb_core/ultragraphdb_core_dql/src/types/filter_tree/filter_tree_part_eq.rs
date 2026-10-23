/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::FilterTree;

impl PartialEq for FilterTree {
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

impl FilterTree {
    /// Whether the nodes are equal, their children aside but for their number. Every field is named, so that a
    /// field added to the type cannot be left out of the comparison.
    fn eq_node(&self, other: &Self) -> bool {
        let FilterTree {
            op: op_a,
            child: child_a,
            func: func_a,
        } = self;
        let FilterTree {
            op: op_b,
            child: child_b,
            func: func_b,
        } = other;
        op_a == op_b && child_a.len() == child_b.len() && func_a == func_b
    }
}
