/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::FilterTree;

impl Drop for FilterTree {
    /// Drops the descendants one at a time, from a stack rather than by recursion, so that dropping a deep tree
    /// takes no more call stack than dropping one node (ledger 32).
    fn drop(&mut self) {
        let mut stack = std::mem::take(&mut self.child);
        while let Some(mut node) = stack.pop() {
            stack.append(&mut node.child);
        }
    }
}
