/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::FilterTree;

impl Clone for FilterTree {
    /// Copies the tree node by node along a path kept on the heap rather than by recursion, so that cloning a
    /// deep tree takes no more call stack than cloning one node (ledger 32).
    fn clone(&self) -> Self {
        // The nodes above the one being copied, each with its copy so far.
        let mut path: Vec<(&FilterTree, FilterTree)> = Vec::new();
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

impl FilterTree {
    /// A copy of the node without its children. Every field is named, so that a field added to the type cannot be
    /// left out of the copy.
    fn clone_node(&self) -> Self {
        let FilterTree { op, child, func } = self;
        FilterTree {
            op: op.clone(),
            child: Vec::with_capacity(child.len()),
            func: func.clone(),
        }
    }
}
