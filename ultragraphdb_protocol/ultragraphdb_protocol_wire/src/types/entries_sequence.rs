/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::entries_end::EntriesEnd;
use crate::types::entries_view::EntriesView;
use crate::types::frame_view::FrameView;
use crate::utils::constants::FLAG_LAST;

/// The frame before the one being checked: what the rules comparing two frames read of it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Previous {
    range: u32,
    end: EntriesEnd,
    continues: bool,
    /// The last key its range has sent so far; `None` before the range's first entry.
    last_key: Option<Vec<u8>>,
}

/// The rules of one Scan's answer that compare its Entries frames (`PROTOCOL.md` §13.14), held by the receiver for the
/// stream, with the frames in the order they arrive. A refusal closes the connection, as any side that dialled does (§2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntriesSequence {
    range_count: u32,
    previous: Option<Previous>,
}

impl EntriesSequence {
    /// The sequence of an answer to a Scan of `range_count` ranges, before its first frame.
    pub fn new(range_count: u32) -> Self {
        Self {
            range_count,
            previous: None,
        }
    }

    /// `frame` read as an Entries frame, checked against the frames before it, and taken as the previous frame; its view.
    /// After the frame's own refusals, in the protocol's order: in a frame naming the previous frame's range, a first key
    /// at or below the last key the range has sent, or, after a frame with `continues` 1, no entry or a first key not
    /// equal to it ([`ProtocolError::field_value`] `"entry order"`, 0); a range other than the one the previous frame's
    /// `end` leads to, range 0 for the first frame, or not below the Scan's range count
    /// ([`ProtocolError::field_value`] `"range"`, the frame's range); `LAST` on any frame but the last range's with `end`
    /// not 0, or its absence from that frame ([`ProtocolError::field_value`] `"end"`, the frame's range).
    pub fn accept<'a>(&mut self, frame: &FrameView<'a>) -> Result<EntriesView<'a>, ProtocolError> {
        let view = EntriesView::from_frame(frame)?;
        let command = Command::Entries.number();
        let range = view.range();
        let mut entries = view.entries().iter();
        let first = entries.next();
        let same_range = self.previous.as_ref().filter(|p| p.range == range);
        let continuing = same_range.is_some_and(|p| p.continues);
        let last_key = same_range.and_then(|p| p.last_key.as_deref());
        let in_order = match (&first, last_key) {
            (None, _) => !continuing,
            (Some(_), None) => true,
            (Some(entry), Some(last)) if continuing => entry.key() == last,
            (Some(entry), Some(last)) => entry.key() > last,
        };
        if !in_order {
            return Err(ProtocolError::field_value(command, "entry order", 0));
        }
        let expected = match &self.previous {
            None => 0,
            Some(previous) if previous.end == EntriesEnd::More => previous.range,
            Some(previous) => previous.range.saturating_add(1),
        };
        if range != expected || range >= self.range_count {
            return Err(ProtocolError::field_value(command, "range", range.into()));
        }
        let ends_answer =
            range.saturating_add(1) == self.range_count && view.end() != EntriesEnd::More;
        if (frame.header().flags() & FLAG_LAST != 0) != ends_answer {
            return Err(ProtocolError::field_value(command, "end", range.into()));
        }
        let range_last_key = entries
            .last()
            .or(first)
            .map(|entry| entry.key().to_vec())
            .or_else(|| last_key.map(<[u8]>::to_vec));
        self.previous = Some(Previous {
            range,
            end: view.end(),
            continues: view.continues(),
            last_key: range_last_key,
        });
        Ok(view)
    }
}
