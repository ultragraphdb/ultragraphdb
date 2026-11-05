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
use crate::types::key_values::KeyValues;
use crate::types::scan_range::ScanRange;
use crate::utils::constants::{DICTIONARY_KEYSPACE, FLAG_LAST};

/// The frame before the one being checked: what the rules comparing two frames read of it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Previous {
    range: u32,
    through: u32,
    end: EntriesEnd,
    continues: bool,
    /// The last key its range has sent so far; `None` before the range's first entry.
    last_key: Option<Vec<u8>>,
}

/// A range of the Scan, its keys copied, as the rules placing a frame's entries read it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyRange {
    first: Vec<u8>,
    past: Vec<u8>,
}

impl KeyRange {
    /// Whether the range is a one-key range of the dictionary: its past key its first key followed by a zero byte.
    fn is_dictionary_key(&self) -> bool {
        self.first.first() == Some(&DICTIONARY_KEYSPACE)
            && self.past.split_last() == Some((&0, self.first.as_slice()))
    }

    /// Whether `key` lies in the range.
    fn holds(&self, key: &[u8]) -> bool {
        key >= self.first.as_slice() && (self.past.is_empty() || key < self.past.as_slice())
    }
}

/// The rules of one Scan's answer that compare its Entries frames with each other and with the Scan's ranges
/// (`PROTOCOL.md` §13.14), held by the receiver for the stream, with the frames in the order they arrive. A refusal closes
/// the connection, as any side that dialled does (§2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntriesSequence {
    ranges: Vec<KeyRange>,
    version: Option<u64>,
    previous: Option<Previous>,
}

impl EntriesSequence {
    /// The sequence of an answer to a Scan of `ranges`, before its first frame.
    pub fn new<'r>(ranges: impl IntoIterator<Item = ScanRange<'r>>) -> Self {
        Self {
            ranges: ranges
                .into_iter()
                .map(|range| KeyRange {
                    first: range.first().to_vec(),
                    past: range.past().to_vec(),
                })
                .collect(),
            version: None,
            previous: None,
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
        self.check_range(&view)?;
        let ends_answer =
            view.through() as usize + 1 == self.ranges.len() && view.end() != EntriesEnd::More;
        if (frame.header().flags() & FLAG_LAST != 0) != ends_answer {
            return Err(ProtocolError::field_value(command, "end", range.into()));
        }
        if *self.version.get_or_insert(view.version()) != view.version() {
            return Err(ProtocolError::field_value(command, "version", range.into()));
        }
        self.check_entries(&view)?;
        let range_last_key = entries
            .last()
            .or(first)
            .map(|entry| entry.key().to_vec())
            .or_else(|| last_key.map(<[u8]>::to_vec));
        self.previous = Some(Previous {
            range,
            through: view.through(),
            end: view.end(),
            continues: view.continues(),
            last_key: range_last_key,
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
        let run = self.run(range, view.through());
        let answered = match run {
            Some([_]) => true,
            Some(run) => run.iter().all(KeyRange::is_dictionary_key),
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
        let run = self.run(view.range(), view.through()).unwrap_or_default();
        let misplaced = match run {
            [range] => outside_range(view.entries(), range),
            _ => outside_run(view.entries(), run),
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

    /// The Scan's ranges from `range` through `through`; `None` past the last.
    fn run(&self, range: u32, through: u32) -> Option<&[KeyRange]> {
        self.ranges.get(range as usize..=through as usize)
    }
}

/// The index of the first of `entries` outside `range`.
fn outside_range(entries: KeyValues<'_>, range: &KeyRange) -> Option<usize> {
    entries.iter().position(|entry| !range.holds(entry.key()))
}

/// The index of the first of `entries` that is not the first key of a range of `run` at or after the range of the entry
/// before.
fn outside_run(entries: KeyValues<'_>, run: &[KeyRange]) -> Option<usize> {
    let mut at = 0;
    entries.iter().position(|entry| {
        let found = run
            .get(at..)
            .and_then(|rest| rest.iter().position(|r| r.first == entry.key()));
        at = at.saturating_add(found.unwrap_or_default());
        found.is_none()
    })
}
