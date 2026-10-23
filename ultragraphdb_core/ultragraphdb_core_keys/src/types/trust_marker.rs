/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The marker a reserved namespace's trusted in-process caller carries to write its value-locked predicates.
///
/// Dgraph's `TrustMarker` is any Go value used as a context key, compared by type and value; this port names a
/// marker by a static string and compares markers by that name, so each owner picks a name no other owner uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrustMarker(&'static str);

impl TrustMarker {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn name(&self) -> &'static str {
        self.0
    }
}
