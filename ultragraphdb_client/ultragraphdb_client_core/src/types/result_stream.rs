/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The result of a Read or an Execute, arriving as Result frames on its transaction's stream, the last flagged; read
/// batch by batch through [`crate::ClientCore::next_batch`]. The handle holds what the core need not: that its read was read
/// to its end, and, by being held, that its open read is still wanted and what a lost connection kept for the read may
/// still be asked for; an open read whose handle is dropped is cancelled before the next read takes its buffers. Its
/// clones are one handle.
#[derive(Debug, Clone)]
pub struct ResultStream {
    stream: u64,
    generation: u64,
    // Shared (Ultra rule 9): the core keeps a clone and counts the `Arc`'s holders to know whether the handle is still
    // held ([`handle_held`]); the flag is an atomic so the handle is `Send` and `Sync`. Relaxed suffices: the flag
    // orders no other memory.
    read_to_end: Arc<AtomicBool>,
}

impl ResultStream {
    pub(crate) fn new(stream: u64, generation: u64) -> Self {
        Self {
            stream,
            generation,
            read_to_end: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The stream the results arrive on.
    #[must_use]
    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// What the core keeps of the handle: [`handle_held`] of it says whether the handle is still held.
    pub(crate) fn tracker(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.read_to_end)
    }

    /// Notes that the read was read to its end.
    pub(crate) fn mark_read_to_end(&self) {
        self.read_to_end.store(true, Ordering::Relaxed);
    }

    /// Whether the read was read to its end.
    pub(crate) fn is_read_to_end(&self) -> bool {
        self.read_to_end.load(Ordering::Relaxed)
    }
}

impl PartialEq for ResultStream {
    fn eq(&self, other: &Self) -> bool {
        (self.stream, self.generation) == (other.stream, other.generation)
    }
}

impl Eq for ResultStream {}

/// Whether a handle `tracker` came from, or a clone of it, is still held: the core's own reference aside.
pub(crate) fn handle_held(tracker: &Arc<AtomicBool>) -> bool {
    Arc::strong_count(tracker) > 1
}
