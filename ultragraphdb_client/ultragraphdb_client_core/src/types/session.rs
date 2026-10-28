/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A session of the client (D9): its number, given by the server's Register, through which every commit is sent and
/// retried, one at a time. A handle: the client keeps the request numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Session {
    number: u64,
}

impl Session {
    pub(crate) fn new(number: u64) -> Self {
        Self { number }
    }

    pub fn number(&self) -> u64 {
        self.number
    }
}
