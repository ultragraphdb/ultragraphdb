/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::GraphQuery;

impl Drop for GraphQuery {
    /// Drops the descendants one at a time, from a stack rather than by recursion, so that dropping a deep tree
    /// takes no more call stack than dropping one block (ledger 32).
    fn drop(&mut self) {
        let mut stack = std::mem::take(&mut self.children);
        while let Some(mut block) = stack.pop() {
            stack.append(&mut block.children);
        }
    }
}
