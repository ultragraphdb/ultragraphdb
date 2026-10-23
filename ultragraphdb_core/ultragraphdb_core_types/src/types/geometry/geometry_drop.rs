/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::Geometry;

impl Drop for Geometry {
    /// Drops the members of nested collections from a stack: GeoJSON nests them up to JSON's 10000 levels and WKB
    /// without a limit, which a recursive drop would overflow a thread's stack with.
    fn drop(&mut self) {
        let mut stack = std::mem::take(&mut self.geoms);
        while let Some(mut g) = stack.pop() {
            stack.append(&mut g.geoms);
        }
    }
}
