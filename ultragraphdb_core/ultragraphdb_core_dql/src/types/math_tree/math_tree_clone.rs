/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::MathTree;

impl Clone for MathTree {
    /// Copies the tree node by node along a path kept on the heap rather than by recursion, so that cloning a
    /// deep tree takes no more call stack than cloning one node (ledger 32).
    fn clone(&self) -> Self {
        // The nodes above the one being copied, each with its copy so far.
        let mut path: Vec<(&MathTree, MathTree)> = Vec::new();
        let mut src = self;
        let mut copy = src.clone_node();
        loop {
            if let Some(child) = src.child.get(copy.child.len()) {
                let child_copy = child.clone_node();
                path.push((src, std::mem::replace(&mut copy, child_copy)));
                src = child;
                continue;
            }
            match path.pop() {
                Some((parent, mut parent_copy)) => {
                    parent_copy.child.push(copy);
                    src = parent;
                    copy = parent_copy;
                }
                None => return copy,
            }
        }
    }
}

impl MathTree {
    /// A copy of the node without its children. Every field is named, so that a field added to the type cannot be
    /// left out of the copy.
    fn clone_node(&self) -> Self {
        let MathTree {
            fn_name,
            var,
            const_val,
            val,
            child,
        } = self;
        MathTree {
            fn_name: fn_name.clone(),
            var: var.clone(),
            const_val: const_val.clone(),
            val: val.clone(),
            child: Vec::with_capacity(child.len()),
        }
    }
}
