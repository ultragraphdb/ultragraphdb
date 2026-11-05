/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{Entries, EntriesEnd, ObjectRecordsBuilder};

/// The bytes of an object record before its encoding: its two lengths.
const RECORD_LENGTHS: usize = 8;
/// The bytes of a statement ID in an object record.
const STATEMENT_ID: usize = 16;

/// One Entries frame being filled up to the receiver's `max_frame` (`PROTOCOL.md` §13.14): what a page offers is taken while
/// the frame it would build fits, by exact sizes, and refused once it would not.
#[derive(Debug)]
pub struct EntriesFill {
    entries: Entries,
    max_frame: usize,
    key: Option<Vec<u8>>,
    value: ObjectRecordsBuilder,
}

impl EntriesFill {
    /// A frame of range `range` read at `epoch`, built to at most `max_frame` bytes, holding nothing yet.
    #[must_use]
    pub fn new(range: u32, epoch: u64, max_frame: usize) -> Self {
        Self {
            entries: Entries::new(range, EntriesEnd::More, epoch),
            max_frame,
            key: None,
            value: ObjectRecordsBuilder::new(),
        }
    }

    /// Takes the object `object` of the list `key`, held under `statements`, unless the frame has no room for its record.
    pub fn offer_object(
        &mut self,
        key: &[u8],
        object: &EncodedTerm,
        statements: &[SystemId],
    ) -> ControlFlow<()> {
        if self.key.as_deref() != Some(key) {
            self.close_key();
        }
        let encoding = object.as_bytes();
        let record = RECORD_LENGTHS + encoding.len() + STATEMENT_ID * statements.len();
        let held = self.value.value().len();
        if self.entries.frame_size_with(key.len(), held + record) > self.max_frame {
            return ControlFlow::Break(());
        }
        if self.key.is_none() {
            self.key = Some(key.to_vec());
        }
        let ids: Vec<[u8; STATEMENT_ID]> = statements.iter().map(|id| id.to_be_bytes()).collect();
        self.value.push(encoding, &ids);
        ControlFlow::Continue(())
    }

    /// Takes the stored `value` of `key` whole, unless the frame has no room for it.
    pub fn offer_value(&mut self, key: &[u8], value: &[u8]) -> ControlFlow<()> {
        self.close_key();
        if self.entries.frame_size_with(key.len(), value.len()) > self.max_frame {
            return ControlFlow::Break(());
        }
        self.entries.push(key, value);
        ControlFlow::Continue(())
    }

    /// Whether the frame took nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.key.is_none() && self.entries.entries().is_empty()
    }

    /// The frame's batch, how its range went on as `end` says.
    #[must_use]
    pub fn finish(mut self, end: EntriesEnd) -> Entries {
        self.close_key();
        self.entries.with_end(end)
    }

    /// The frame's batch, its last entry's value continuing in the next frame.
    #[must_use]
    pub fn finish_continuing(mut self) -> Entries {
        self.close_key();
        self.entries.continued()
    }

    /// Moves what the key under way gathered into the batch.
    fn close_key(&mut self) {
        if let Some(key) = self.key.take() {
            self.entries.push(&key, self.value.value());
            self.value = ObjectRecordsBuilder::new();
        }
    }
}
