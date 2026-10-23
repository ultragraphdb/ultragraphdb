/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::slice::Iter;

use super::Geometry;

impl Geometry {
    /// A copy of everything but the members.
    fn shallow_clone(&self) -> Geometry {
        Geometry {
            kind: self.kind,
            layout: self.layout,
            flat_coords: self.flat_coords.clone(),
            ends: self.ends.clone(),
            endss: self.endss.clone(),
            geoms: Vec::with_capacity(self.geoms.len()),
        }
    }
}

impl Clone for Geometry {
    /// Copies the members of nested collections from a stack, for the depth [`Drop`] handles.
    fn clone(&self) -> Self {
        // Each open collection's copy so far, with the members left to copy; the root's is the first.
        let mut stack: Vec<(Geometry, Iter<'_, Geometry>)> =
            vec![(self.shallow_clone(), self.geoms.iter())];
        while let Some((_, members)) = stack.last_mut() {
            if let Some(m) = members.next() {
                stack.push((m.shallow_clone(), m.geoms.iter()));
                continue;
            }
            let Some((done, _)) = stack.pop() else {
                break;
            };
            match stack.last_mut() {
                Some((parent, _)) => parent.geoms.push(done),
                None => return done,
            }
        }
        // The loop returns the root's copy when it closes; the stack is never empty before.
        self.shallow_clone()
    }
}
