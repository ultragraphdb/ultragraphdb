/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An answer carried in continuation frames (`PROTOCOL.md` §8.3, §13.7): each frame holds the answer's command header
//! and a part of every section, the parts of a section concatenating in frame order to the whole, the last frame alone
//! flagged `LAST`. A frame splits a section of records or u32s only between whole ones and a text only on a character
//! boundary.

use std::num::NonZeroUsize;
use std::ops::Range;

use ultragraphdb_core_body::{align8, write_sections};
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::chunk_layout::ChunkLayout;
use crate::types::header::Header;
use crate::utils::constants::{
    CHUNK_SIZE, FIRST_CHUNK_SIZE, FLAG_LAST, HEADER_SIZE, LEAF_SIZE, MAX_FRAME_SIZE,
};

/// Where a frame may end its part of a section.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Cut<'a> {
    /// Between whole units of this many bytes: u32s or records.
    Units(usize),
    /// On a character boundary of the section's UTF-8 text.
    Text,
    /// Anywhere outside the ranges, which hold texts, and on a character boundary inside them.
    Texts(&'a [Range<usize>]),
}

impl Cut<'_> {
    /// The bytes of the unit that begins at `start` in `bytes`: a record or a u32, or a character inside a text.
    fn unit(self, bytes: &[u8], start: usize) -> usize {
        match self {
            Cut::Units(unit) => unit.max(1),
            Cut::Text => char_len(bytes, start),
            Cut::Texts(ranges) if ranges.iter().any(|r| r.contains(&start)) => {
                char_len(bytes, start)
            }
            Cut::Texts(_) => 1,
        }
    }

    /// The end of the part from `start`, at most `limit`, where this cut allows a frame to end it in `bytes`; `None`
    /// for a `limit` below `start`.
    fn end(self, bytes: &[u8], start: usize, limit: usize) -> Option<usize> {
        Some(match self {
            Cut::Units(unit) => {
                // Whole units from `start`: the span rounded down to a multiple of the unit, within `limit`.
                let unit = NonZeroUsize::new(unit).unwrap_or(NonZeroUsize::MIN);
                let span = limit.checked_sub(start)?;
                start.checked_add(span.checked_sub(span % unit)?)?
            }
            Cut::Text => char_boundary(bytes, start, limit),
            Cut::Texts(ranges) => match ranges.iter().find(|r| r.start < limit && limit < r.end) {
                Some(range) => char_boundary(bytes, start.max(range.start), limit),
                None => limit,
            },
        })
    }
}

/// The bytes of the character that begins at `start` in `bytes`: its first byte and the continuation bytes after it, at
/// most three.
fn char_len(bytes: &[u8], start: usize) -> usize {
    let continuing = bytes
        .get(start..)
        .and_then(|rest| rest.get(1..))
        .unwrap_or_default()
        .iter()
        .take(3)
        .take_while(|byte| *byte & 0xC0 == 0x80)
        .count();
    // At most three continuation bytes, so the length never saturates.
    continuing.saturating_add(1)
}

/// The last character boundary of `bytes` at or below `limit` and at or above `floor`: a byte that is no UTF-8
/// continuation byte starts a character, and a character takes at most four bytes.
fn char_boundary(bytes: &[u8], floor: usize, limit: usize) -> usize {
    let mut end = limit;
    for _ in 0..4 {
        let continues = bytes.get(end).is_some_and(|byte| byte & 0xC0 == 0x80);
        if end <= floor || !continues {
            return end;
        }
        // `end` is above `floor`, so it is at least 1.
        let Some(before) = end.checked_sub(1) else {
            return end;
        };
        end = before;
    }
    end.max(floor)
}

/// The bytes a frame of `body_len` bytes of body takes, its header and chunk table included.
pub(crate) fn frame_len(body_len: usize) -> usize {
    HEADER_SIZE.saturating_add(ChunkLayout::of_data(body_len).body_len())
}

/// The most bytes of body, before its chunk table, a frame within `max_frame` holds: the body of one chunk, or of `n ≥ 2`
/// chunks, which hold at most 3,840 + 4,096 (n − 1) bytes beside a table of 8n. `None` past `usize`, which a room of at
/// most 1 MiB never reaches.
pub(crate) fn data_room(max_frame: usize) -> Option<usize> {
    let room = max_frame.min(MAX_FRAME_SIZE).saturating_sub(HEADER_SIZE);
    if room <= FIRST_CHUNK_SIZE {
        return Some(room);
    }
    // The chunks' room and the table's meet near n = (room + 256) / 4,104; either side of it is the most.
    let near = room.checked_add(CHUNK_SIZE - FIRST_CHUNK_SIZE)? / (CHUNK_SIZE + LEAF_SIZE);
    [Some(near), near.checked_add(1)]
        .into_iter()
        .filter(|chunks| chunks.is_none_or(|chunks| chunks >= 2))
        .map(|chunks| {
            let chunks = chunks?;
            let chunks_room = CHUNK_SIZE
                .checked_mul(chunks.checked_sub(1)?)?
                .checked_add(FIRST_CHUNK_SIZE)?;
            Some(
                room.checked_sub(LEAF_SIZE.checked_mul(chunks)?)?
                    .min(chunks_room),
            )
        })
        .try_fold(FIRST_CHUNK_SIZE, |most, room| Some(most.max(room?)))
}

/// The frames that carry the body of `sections` within `max_frame`, each under `header`, whose command header every
/// frame holds: one frame when the body fits, else as many as it takes, each section cut where its [`Cut`] allows,
/// the last frame alone flagged `LAST`.
///
/// # Errors
///
/// [`ProtocolError::frame_too_large`] when a frame within `max_frame` carries no unit of what is left, naming the size of
/// [`least_frame`].
pub(crate) fn continuation_frames(
    header: Header,
    max_frame: usize,
    sections: &[(&[u8], Cut<'_>)],
) -> Result<Vec<Vec<u8>>, ProtocolError> {
    let whole: Vec<&[u8]> = sections.iter().map(|(bytes, _)| *bytes).collect();
    let body = write_sections(&[], &whole);
    let flags = header.flags();
    if frame_len(body.len()) <= max_frame {
        return Ok(vec![
            header.with_flags(flags | FLAG_LAST).encode_frame(&body)?,
        ]);
    }
    let room = data_room(max_frame)
        .ok_or_else(|| ProtocolError::frame_too_large(frame_len(body.len()), max_frame))?;
    let total = whole.iter().map(|bytes| bytes.len()).sum::<usize>();
    let mut cursors = vec![0usize; sections.len()];
    let mut frames = Vec::new();
    // Each frame carries at least one byte, so the answer takes at most one frame a byte.
    for _ in 0..=total {
        let parts = next_parts(sections, &mut cursors, room).ok_or_else(|| {
            ProtocolError::frame_too_large(least_frame(sections, &cursors), max_frame)
        })?;
        let carried = parts.iter().map(|part| part.len()).sum::<usize>();
        if carried == 0 {
            return Err(ProtocolError::frame_too_large(
                least_frame(sections, &cursors),
                max_frame,
            ));
        }
        let done = cursors
            .iter()
            .zip(&whole)
            .all(|(cursor, bytes)| *cursor == bytes.len());
        let frame_flags = if done {
            flags | FLAG_LAST
        } else {
            flags & !FLAG_LAST
        };
        let frame_header = header.clone().with_flags(frame_flags);
        frames.push(frame_header.encode_frame(&write_sections(&[], &parts))?);
        if done {
            return Ok(frames);
        }
    }
    Err(ProtocolError::frame_too_large(total, max_frame))
}

/// The least frame that holds the next unit of a section with bytes left, the least over those sections, which a frame
/// carries whichever it holds (`PROTOCOL.md` §2.3, §4.1): the frame header and the command header, 256 bytes; the section
/// table; and the unit at the start of its section, padded to 8 for the empty sections after it unless its section is
/// the last. It is one chunk, with no chunk table.
fn least_frame(sections: &[(&[u8], Cut<'_>)], cursors: &[usize]) -> usize {
    let count = sections.len();
    let last = count.checked_sub(1);
    sections
        .iter()
        .zip(cursors)
        .enumerate()
        .filter(|(_, ((bytes, _), cursor))| **cursor < bytes.len())
        .map(|(index, ((bytes, cut), cursor))| {
            let unit = cut.unit(bytes, *cursor);
            let held = if Some(index) == last {
                unit
            } else {
                align8(unit)
            };
            // A frame's few sections and one unit: the size never saturates.
            HEADER_SIZE
                .saturating_add(count.saturating_mul(8))
                .saturating_add(held)
        })
        .min()
        .unwrap_or(HEADER_SIZE)
}

/// The next frame's part of each section, from its cursor, within `room` bytes of body: each section after the one
/// before rounded up to 8, and each but the last ending by `room` rounded down to 8, so the padding after it fits.
/// Moves the cursors past the parts. `None` for a place past `usize`, which a frame's body of at most 1 MiB never
/// reaches.
fn next_parts<'a>(
    sections: &[(&'a [u8], Cut<'_>)],
    cursors: &mut [usize],
    room: usize,
) -> Option<Vec<&'a [u8]>> {
    let count = sections.len();
    let last = count.checked_sub(1);
    let mut at = count.checked_mul(8)?;
    let mut parts = Vec::with_capacity(count);
    for (index, ((bytes, cut), cursor)) in sections.iter().zip(cursors.iter_mut()).enumerate() {
        at = align8(at);
        let end_by = if Some(index) == last { room } else { room & !7 };
        let space = end_by.saturating_sub(at);
        let start = *cursor;
        let limit = start.saturating_add(space).min(bytes.len());
        let end = cut.end(bytes, start, limit)?;
        parts.push(bytes.get(start..end)?);
        at = at.checked_add(end.checked_sub(start)?)?;
        *cursor = end;
    }
    Some(parts)
}
