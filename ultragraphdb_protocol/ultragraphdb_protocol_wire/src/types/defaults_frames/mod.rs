/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::defaults_view::{DefaultsView, follows};
use crate::types::frame_view::FrameView;
use crate::utils::command_header::expect_command;
use crate::utils::constants::{CATALOG_BYTES_MAX, COMMAND_HEADER_SIZE, FLAG_LAST};
use crate::utils::le::bytes_at;

/// A Defaults reply's frames read as one answer (`PROTOCOL.md` §13.28): predicates that do not fit one frame within
/// the requester's `max_frame` continue in further Defaults frames on the stream, each with the first's command header
/// and a whole term table, the last alone flagged `LAST`; the entries' order and an entry's index run on across the
/// frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultsFrames {
    command_header: Option<[u8; COMMAND_HEADER_SIZE]>,
    predicates: Vec<String>,
    last: bool,
    frames: u64,
    bytes: usize,
    bytes_max: usize,
    past_bound: bool,
}

impl DefaultsFrames {
    /// No frame taken yet; the answer's bytes bounded by [`crate::CATALOG_BYTES_MAX`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_bytes_max(CATALOG_BYTES_MAX)
    }

    /// No frame taken yet; the answer's bytes bounded by `bytes_max`, the receiver's `catalog_bytes_max`.
    #[must_use]
    pub fn with_bytes_max(bytes_max: usize) -> Self {
        Self {
            command_header: None,
            predicates: Vec::new(),
            last: false,
            frames: 0,
            bytes: 0,
            bytes_max,
            past_bound: false,
        }
    }

    /// Takes `frame`, the reply's next frame. It is checked by its own rules first, through
    /// [`crate::DefaultsView`], an entry's index running on from the frames before; then against the frames before:
    /// no frame taken flagged `LAST`, and its command header the first frame's ([`ProtocolError::field_value`]
    /// `"defaults"`, naming the frames taken); then its first entry above the last entry taken, by its text's bytes
    /// ([`ProtocolError::field_value`] `"default order"`, the entry's index).
    ///
    /// A frame that takes the term tables' bytes, sections 0 and 1 of every frame taken, past the bound is not
    /// read, and no frame after it: [`DefaultsFrames::is_past_bound`] then answers true and the predicates stay as
    /// they were.
    ///
    /// # Errors
    ///
    /// The refusal of the frame's view, a command header other than the first frame's, or an entry out of order.
    pub fn push(&mut self, frame: &FrameView<'_>) -> Result<(), ProtocolError> {
        if self.past_bound {
            return Ok(());
        }
        // Lossless: a `usize` is at most 64 bits.
        let taken = self.predicates.len() as u64;
        let fields = expect_command(frame, Command::Defaults)?;
        let view = DefaultsView::read(fields, frame.body(), taken)?;
        let header = frame.header().command_header();
        let other = self.command_header.is_some_and(|first| first != *header);
        if self.last || other {
            return Err(ProtocolError::field_value(
                Command::Defaults.number(),
                "defaults",
                self.frames,
            ));
        }
        let before = self.predicates.last().map(String::as_str);
        follows(before, view.predicate(0), taken)?;
        let held = self.bytes.saturating_add(view.table_bytes());
        if held > self.bytes_max {
            self.past_bound = true;
            return Ok(());
        }
        self.predicates.extend(
            (0..view.len())
                .filter_map(|index| view.predicate(index))
                .map(str::to_string),
        );
        self.bytes = held;
        self.command_header = Some(*header);
        self.frames = self.frames.saturating_add(1);
        self.last = frame.header().flags() & FLAG_LAST != 0;
        Ok(())
    }

    /// Whether the frame flagged `LAST` has been taken: the answer is whole.
    #[must_use]
    pub fn is_last(&self) -> bool {
        self.last
    }

    /// Whether the answer's bytes passed the bound: the requester reads no further frame of it.
    #[must_use]
    pub fn is_past_bound(&self) -> bool {
        self.past_bound
    }

    /// The tenant the first frame names; `None` before the first frame.
    #[must_use]
    pub fn tenant(&self) -> Option<u64> {
        self.field(0)
    }

    /// The log position the first frame names; `None` before the first frame.
    #[must_use]
    pub fn position(&self) -> Option<u64> {
        self.field(8)
    }

    /// The predicates of the frames taken, in order.
    #[must_use]
    pub fn predicates(&self) -> &[String] {
        &self.predicates
    }

    /// The u64 at `at` of the first frame's command header; `None` before the first frame.
    fn field(&self, at: usize) -> Option<u64> {
        let header = self.command_header.as_ref()?;
        bytes_at(header, at).map(u64::from_le_bytes)
    }
}

impl Default for DefaultsFrames {
    fn default() -> Self {
        Self::new()
    }
}
