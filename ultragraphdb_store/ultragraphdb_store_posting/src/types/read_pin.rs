/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::lists::Lists;
use crate::utils::lock::lock;

/// An open read at one version with no request behind it ([`Lists::pin`]): while it lives the discard timestamp stays at or
/// below its version, so a read spanning many turns reads that version throughout. Dropping it closes the read.
#[derive(Debug)]
pub struct ReadPin<'a, S, C> {
    lists: &'a Lists<S, C>,
    version: u64,
}

impl<'a, S, C> ReadPin<'a, S, C> {
    /// The pin of a read `lists` has already recorded open at `version`.
    pub(crate) fn new(lists: &'a Lists<S, C>, version: u64) -> Self {
        Self { lists, version }
    }

    /// The version kept readable.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }
}

impl<S, C> Drop for ReadPin<'_, S, C> {
    /// Closes the read, which no longer bounds the discard timestamp.
    fn drop(&mut self) {
        lock(self.lists.state()).ts.close_read(self.version);
    }
}
