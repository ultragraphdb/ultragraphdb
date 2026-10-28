/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The result of a Read or an Execute, arriving as Result frames on its transaction's stream, the last flagged; read
/// batch by batch through [`crate::ClientCore::next_batch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResultStream {
    stream: u64,
    generation: u64,
}

impl ResultStream {
    pub(crate) fn new(stream: u64, generation: u64) -> Self {
        Self { stream, generation }
    }

    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}
