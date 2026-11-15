/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod defaults_body_builder;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::defaults::defaults_body_builder::{body_len, table_body, table_len};
use crate::types::defaults_view::{DefaultsView, follows};
use crate::types::header::Header;
use crate::utils::command_header::CommandHeader;
use crate::utils::constants::FLAG_LAST;
use crate::utils::continuation::frame_len;

/// The default-entry records the metadata state machine holds for a tenant (`PROTOCOL.md` §13.28): the predicates
/// holding a record, each an IRI of the term table, in the order of their texts' UTF-8 bytes. A body with `position` 0
/// is a request.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `tenant` | u64 |
/// | 8 | `position` | u64: 0 in a request; in a reply the metadata state machine's log position the records were read at, from 1 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the term table's records: one IRI per predicate holding a record; empty in a request |
/// | 1 | the term table's text |
///
/// [`crate::BodyBuilder::frame`] and [`Defaults::frames`] refuse what [`crate::DefaultsView`] and
/// [`crate::DefaultsFrames`] refuse: a predicate in a body of `position` 0, a predicate repeated within a frame, and
/// one at or below the one before by its text's bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Defaults {
    tenant: u64,
    position: u64,
    predicates: Vec<String>,
}

impl Defaults {
    /// The request for `tenant`'s default-entry predicates.
    #[must_use]
    pub fn request(tenant: u64) -> Self {
        Self {
            tenant,
            position: 0,
            predicates: Vec::new(),
        }
    }

    /// The reply: `tenant`'s `predicates` as read at the log `position`, ascending by their texts' bytes.
    #[must_use]
    pub fn reply(tenant: u64, position: u64, predicates: Vec<String>) -> Self {
        Self {
            tenant,
            position,
            predicates,
        }
    }

    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The log position the records were read at; 0 in a request.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The predicates holding a record, in order; empty in a request.
    #[must_use]
    pub fn predicates(&self) -> &[String] {
        &self.predicates
    }

    /// The reply as the frames that carry it within `max_frame` (`PROTOCOL.md` §13.28): one frame when its predicates
    /// fit, else as many as they take, each holding a whole term table and this command header, the last alone
    /// flagged `LAST`. `header` gives every frame's other fields.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::frame_too_large`] when a frame within `max_frame` does not hold the next predicate, naming
    /// the least frame that does; and what [`crate::DefaultsFrames`] refuses of the frames.
    pub fn frames(&self, header: Header, max_frame: usize) -> Result<Vec<Vec<u8>>, ProtocolError> {
        let command_header = self.command_header();
        let header = header
            .with_command(Command::Defaults)
            .with_command_header(command_header);
        let flags = header.flags();
        let mut frames = Vec::new();
        let mut rest = self.predicates.as_slice();
        let mut before: Option<&str> = None;
        // Each frame but the last holds at least one predicate: at most one frame a predicate, and the last.
        for _ in 0..=self.predicates.len() {
            let (part, after) = cut(rest, max_frame)?;
            // The entries before this frame: lossless, a `usize` is at most 64 bits.
            let taken = self.predicates.len().saturating_sub(rest.len()) as u64;
            let body = table_body(part);
            let view = DefaultsView::read(CommandHeader(&command_header), &body, taken)?;
            follows(before, view.predicate(0), taken)?;
            let last = after.is_empty();
            let frame_flags = if last {
                flags | FLAG_LAST
            } else {
                flags & !FLAG_LAST
            };
            frames.push(header.clone().with_flags(frame_flags).encode_frame(&body)?);
            if last {
                return Ok(frames);
            }
            before = part.last().map(String::as_str).or(before);
            rest = after;
        }
        Err(ProtocolError::frame_too_large(
            frame_len(body_len(rest)),
            max_frame,
        ))
    }
}

/// The predicates of `rest` one frame within `max_frame` holds, as many as fit, and those after them.
///
/// # Errors
///
/// [`ProtocolError::frame_too_large`] when the frame holds none of what is left, naming the least frame that holds
/// the next predicate.
fn cut(rest: &[String], max_frame: usize) -> Result<(&[String], &[String]), ProtocolError> {
    // The text of the predicates taken so far is summed once: a frame's body is its tables and that text.
    let mut text = 0usize;
    let mut fitting = 0usize;
    for (predicate, count) in rest.iter().zip(1usize..) {
        text = text.saturating_add(predicate.len());
        if frame_len(table_len(count, text)) > max_frame {
            break;
        }
        fitting = count;
    }
    match rest.split_at_checked(fitting) {
        Some((part, after)) if fitting > 0 || rest.is_empty() => Ok((part, after)),
        Some(_) | None => Err(ProtocolError::frame_too_large(
            frame_len(body_len(rest.get(..1).unwrap_or_default())),
            max_frame,
        )),
    }
}
