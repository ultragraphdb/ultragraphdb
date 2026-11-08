/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A request waiting for its answer, which [`crate::ClientCore::take_reply`] gives once it has arrived, or which
/// [`crate::ClientCore::forget`] lets go once its caller no longer waits for it. A commit keeps its handle across the
/// reconnects that retry it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pending {
    id: u64,
}

impl Pending {
    pub(crate) fn new(id: u64) -> Self {
        Self { id }
    }

    /// The request's id, unique on the client.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }
}
