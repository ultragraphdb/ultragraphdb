/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::defaults::Defaults;
use crate::types::defaults_view::{DefaultsView, out_of_order};
use crate::types::frame_view::FrameView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::body::read_sections;
use crate::utils::command_header::{CommandHeader, expect_command};
use crate::utils::fields::count;
use crate::utils::term_kind::term_kind;

/// The entries of a reply's term table, each in order: of kind 1, and above the one before by its text's bytes,
/// refused naming its index in the answer, `entries_before` and its index in the frame.
fn check_entries(
    command: u16,
    terms: &TermTableView<'_>,
    entries_before: u64,
) -> Result<(), ProtocolError> {
    let mut before: Option<&str> = None;
    for index in 0..count(command, "defaults", terms.len())? {
        let Some(term) = terms.get(index) else {
            return Ok(());
        };
        let RequestTermView::Iri(iri) = term else {
            return Err(ProtocolError::field_value(
                command,
                "default",
                term_kind(&term).into(),
            ));
        };
        if before.is_some_and(|before| iri.as_bytes() <= before.as_bytes()) {
            return Err(out_of_order(entries_before.saturating_add(index.into())));
        }
        before = Some(iri);
    }
    Ok(())
}

impl<'a> DefaultsView<'a> {
    /// The body of `header` and `body`, checked as [`BodyView::from_frame`] states, an entry out of order named by
    /// its index in the answer: `entries_before`, the entries of the frames before this one, and its index here.
    pub(crate) fn read(
        header: CommandHeader<'a>,
        body: &'a [u8],
        entries_before: u64,
    ) -> Result<Self, ProtocolError> {
        let command = Command::Defaults.number();
        header.unused_zero(&[(0, 16)])?;
        let sections = read_sections::<2>(command, body, 0)?;
        let (records, text) = (sections.get(0), sections.get(1));
        let terms = TermTableView::read(command, (0, 1), sections.offset(0), records, text)?;
        let position = header.u64_at(8);
        if position != 0 {
            check_entries(command, &terms, entries_before)?;
        } else if !terms.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "defaults",
                terms.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        }
        Ok(Self {
            tenant: header.u64_at(0),
            position,
            terms,
            table_bytes: records.len().saturating_add(text.len()),
        })
    }
}

impl<'a> BodyView<'a> for DefaultsView<'a> {
    const COMMAND: Command = Command::Defaults;

    type Body = Defaults;

    /// Checks, in `PROTOCOL.md` §13.28's order: unused bytes 16–127 zero; the sections; the term table; in a request,
    /// no entry ([`ProtocolError::count_mismatch`] `"defaults"`, the entries, 0); in a reply, each entry of kind 1
    /// ([`ProtocolError::field_value`] `"default"`, the entry's kind) and above the one before by its text's bytes
    /// ([`ProtocolError::field_value`] `"default order"`, the entry's index in this frame).
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(expect_command(frame, Self::COMMAND)?, frame.body(), 0)
    }

    fn to_body(&self) -> Result<Defaults, ProtocolError> {
        let predicates = (0..self.len())
            .filter_map(|index| self.predicate(index))
            .map(str::to_string)
            .collect();
        Ok(if self.is_request() {
            Defaults::request(self.tenant)
        } else {
            Defaults::reply(self.tenant, self.position, predicates)
        })
    }
}
