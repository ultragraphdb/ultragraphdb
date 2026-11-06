/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::RangeInclusive;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::entries_end::EntriesEnd;
use crate::types::entries_view::EntriesView;
use crate::types::frame_view::FrameView;
use crate::types::key_range::KeyRange;
use crate::types::key_values::KeyValues;
use crate::types::previous::Previous;
use crate::types::scan_range::ScanRange;
use crate::utils::constants::FLAG_LAST;

/// The rules of one Scan's answer that compare its Entries frames with each other and with the Scan's ranges
/// (`PROTOCOL.md` §13.14), held by the receiver for the stream, with the frames in the order they arrive. A refusal closes
/// the connection, as any side that dialled does (§2.6). The Scan's keys are copied once into one buffer, and the last key
/// a range has sent is kept in a buffer reused from frame to frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntriesSequence {
    /// Every range's first key, then its past key, back to back, in range order.
    keys: Vec<u8>,
    /// For each range, where its first key ends and its past key ends in `keys`; its first key starts where the range
    /// before ends.
    bounds: Vec<(usize, usize)>,
    version: Option<u64>,
    previous: Option<Previous>,
    /// The last key the previous frame's range has sent so far, when `has_last_key`.
    last_key: Vec<u8>,
    /// Whether the previous frame's range has sent an entry.
    has_last_key: bool,
}

impl EntriesSequence {
    /// The sequence of an answer to a Scan of `ranges`, before its first frame.
    pub fn new<'r>(ranges: impl IntoIterator<Item = ScanRange<'r>>) -> Self {
        let mut keys = Vec::new();
        let bounds = ranges
            .into_iter()
            .map(|range| {
                keys.extend_from_slice(range.first());
                let first_end = keys.len();
                keys.extend_from_slice(range.past());
                (first_end, keys.len())
            })
            .collect();
        Self {
            keys,
            bounds,
            version: None,
            previous: None,
            last_key: Vec::new(),
            has_last_key: false,
        }
    }

    /// `frame` read as an Entries frame, checked against the frames before it and the Scan's ranges, and taken as the
    /// previous frame; its view. After the frame's own refusals, in the protocol's order: in a frame naming the previous
    /// frame's range, a first key at or below the last key the range has sent, or, after a frame with `continues` 1, no
    /// entry or a first key not equal to it ([`ProtocolError::field_value`] `"entry order"`, 0); a `range` other than the
    /// one the previous frame leads to, range 0 for the first frame, a `through` not below the Scan's range count, or a run
    /// over a range that is not a one-key range of the dictionary ([`ProtocolError::field_value`] `"range"`, the frame's
    /// range); `LAST` on any frame but the one whose `through` is the last range with `end` not 0, or its absence from that
    /// frame ([`ProtocolError::field_value`] `"end"`, the frame's range); a `version` that is not the first frame's
    /// ([`ProtocolError::field_value`] `"version"`, the frame's range); an entry that does not belong where the frame puts
    /// it ([`ProtocolError::field_value`] `"entry order"`, the entry's index).
    pub fn accept<'a>(&mut self, frame: &FrameView<'a>) -> Result<EntriesView<'a>, ProtocolError> {
        let view = EntriesView::from_frame(frame)?;
        let command = Command::Entries.number();
        let range = view.range();
        let mut entries = view.entries().iter();
        let first = entries.next();
        let same_range = self.previous.is_some_and(|p| p.range == range);
        let continuing = same_range && self.previous.is_some_and(|p| p.continues);
        let last_key = (same_range && self.has_last_key).then_some(self.last_key.as_slice());
        let in_order = match (&first, last_key) {
            (None, _) => !continuing,
            (Some(_), None) => true,
            (Some(entry), Some(last)) if continuing => entry.key() == last,
            (Some(entry), Some(last)) => entry.key() > last,
        };
        if !in_order {
            return Err(ProtocolError::field_value(command, "entry order", 0));
        }
        self.check_range(&view)?;
        let ends_answer =
            view.through() as usize + 1 == self.bounds.len() && view.end() != EntriesEnd::More;
        if (frame.header().flags() & FLAG_LAST != 0) != ends_answer {
            return Err(ProtocolError::field_value(command, "end", range.into()));
        }
        if *self.version.get_or_insert(view.version()) != view.version() {
            return Err(ProtocolError::field_value(command, "version", range.into()));
        }
        self.check_entries(&view)?;
        // A frame of the same range holding no entry keeps the range's last key.
        match entries.last().or(first) {
            Some(entry) => {
                self.last_key.clear();
                self.last_key.extend_from_slice(entry.key());
                self.has_last_key = true;
            }
            None => self.has_last_key &= same_range,
        }
        self.previous = Some(Previous {
            range,
            through: view.through(),
            end: view.end(),
            continues: view.continues(),
        });
        Ok(view)
    }

    /// The frame's `range` the expected one, and its `through` a range of the Scan, every range of a run a one-key range
    /// of the dictionary.
    fn check_range(&self, view: &EntriesView<'_>) -> Result<(), ProtocolError> {
        let range = view.range();
        let expected = match &self.previous {
            None => 0,
            Some(previous) if previous.end == EntriesEnd::More => previous.range,
            Some(previous) => previous.through.saturating_add(1),
        };
        let answered = match self.run(range, view.through()) {
            Some(run) if run.start() == run.end() => true,
            Some(mut run) => run.all(|index| {
                self.key_range(index)
                    .is_some_and(|key_range| key_range.is_dictionary_key())
            }),
            None => false,
        };
        if range != expected || !answered {
            return Err(ProtocolError::field_value(
                Command::Entries.number(),
                "range",
                range.into(),
            ));
        }
        Ok(())
    }

    /// Each entry where the frame puts it: in a run, the first key of a range at or after the range of the entry before;
    /// in a batch of one range, within that range.
    fn check_entries(&self, view: &EntriesView<'_>) -> Result<(), ProtocolError> {
        let run = self.run(view.range(), view.through());
        let single = run
            .as_ref()
            .filter(|run| run.start() == run.end())
            .and_then(|run| self.key_range(*run.start()));
        let misplaced = match (single, run) {
            (Some(range), _) => outside_range(view.entries(), &range),
            (None, Some(run)) => self.outside_run(view.entries(), run),
            (None, None) => view.entries().iter().next().map(|_| 0),
        };
        match misplaced {
            Some(index) => Err(ProtocolError::field_value(
                Command::Entries.number(),
                "entry order",
                index as u64,
            )),
            None => Ok(()),
        }
    }

    /// The indices of the Scan's ranges from `range` through `through`; `None` past the last.
    fn run(&self, range: u32, through: u32) -> Option<RangeInclusive<usize>> {
        let (range, through) = (range as usize, through as usize);
        (range <= through && through < self.bounds.len()).then_some(range..=through)
    }

    /// Range `index` of the Scan; `None` past the last.
    fn key_range(&self, index: usize) -> Option<KeyRange<'_>> {
        let (first_end, past_end) = *self.bounds.get(index)?;
        let start = match index.checked_sub(1) {
            Some(before) => self.bounds.get(before)?.1,
            None => 0,
        };
        Some(KeyRange {
            first: self.keys.get(start..first_end)?,
            past: self.keys.get(first_end..past_end)?,
        })
    }

    /// The index of the first of `entries` that is not the first key of a range of `run` at or after the range of the
    /// entry before.
    fn outside_run(&self, entries: KeyValues<'_>, run: RangeInclusive<usize>) -> Option<usize> {
        let (mut at, end) = (*run.start(), *run.end());
        entries.iter().position(|entry| {
            let found = (at..=end).find(|&index| {
                self.key_range(index)
                    .is_some_and(|key_range| key_range.first == entry.key())
            });
            at = found.unwrap_or(at);
            found.is_none()
        })
    }
}

/// The index of the first of `entries` outside `range`.
fn outside_range(entries: KeyValues<'_>, range: &KeyRange<'_>) -> Option<usize> {
    entries.iter().position(|entry| !range.holds(entry.key()))
}
