/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::default_entry::DefaultEntry;
use crate::types::default_entry_view::DefaultEntryView;
use crate::types::frame_view::FrameView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::fields::boolean;
use crate::utils::sections::{Sections, expect_command};
use crate::utils::term_kind::term_kind;

impl<'a> BodyView<'a> for DefaultEntryView<'a> {
    const COMMAND: Command = Command::DefaultEntry;

    type Body = DefaultEntry;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 9)])?;
        let found = boolean(command, "found", header.u8_at(8))?;
        let sections = Sections::<2>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (0, 1),
            sections.offset(0),
            sections.get(0),
            sections.get(1),
        )?;
        if terms.len() != 1 {
            return Err(ProtocolError::count_mismatch(
                command,
                "default entry",
                terms.len() as u64, // lossless: a `usize` is at most 64 bits
                1,
            ));
        }
        let predicate = match terms.get(0) {
            Some(RequestTermView::Iri(iri)) => iri,
            other => {
                let found_kind = other.as_ref().map_or(0, term_kind);
                return Err(ProtocolError::field_value(
                    command,
                    "default entry",
                    found_kind.into(),
                ));
            }
        };
        Ok(Self {
            tenant: header.u64_at(0),
            found,
            predicate,
        })
    }

    fn to_body(&self) -> Result<DefaultEntry, ProtocolError> {
        Ok(DefaultEntry::request(self.tenant, self.predicate).with_found(self.found))
    }
}
