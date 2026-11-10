/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::purge::Purge;
use crate::types::purge_view::PurgeView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::body::read_sections;
use crate::utils::command_header::expect_command;
use crate::utils::constants::PURGE_IRIS_MAX;
use crate::utils::fields;
use crate::utils::term_kind::term_kind;

impl<'a> BodyView<'a> for PurgeView<'a> {
    const COMMAND: Command = Command::Purge;

    type Body = Purge;

    /// Checks, in `PROTOCOL.md` §8.5's order: unused bytes 20–127 zero; `epoch`, 0 in a request and an epoch in a
    /// reply; `removed`, 0 in a request; the sections; the term table; 1 to 10,000 entries ([`ProtocolError::count_mismatch`]
    /// `"purge"`), each of kind 1 ([`ProtocolError::field_value`] `"purge term"`, the entry's kind); `removed` at most
    /// the entries' count ([`ProtocolError::field_value`] `"removed"`).
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 20)])?;
        let epoch = fields::epoch(command, "epoch", header.u64_at(8), true)?;
        let removed = header.u32_at(16);
        if epoch == 0 && removed != 0 {
            return Err(ProtocolError::field_value(
                command,
                "removed",
                removed.into(),
            ));
        }
        let sections = read_sections::<2>(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (0, 1),
            sections.offset(0),
            sections.get(0),
            sections.get(1),
        )?;
        let count = terms.len();
        if !(1..=PURGE_IRIS_MAX).contains(&count) {
            // The casts are lossless: a `usize` is at most 64 bits.
            return Err(ProtocolError::count_mismatch(
                command,
                "purge",
                count as u64,
                count.clamp(1, PURGE_IRIS_MAX) as u64,
            ));
        }
        if let Some(other) = (0..terms.len())
            .filter_map(|index| terms.get(u32::try_from(index).ok()?))
            .find(|term| !matches!(term, RequestTermView::Iri(_)))
        {
            return Err(ProtocolError::field_value(
                command,
                "purge term",
                term_kind(&other).into(),
            ));
        }
        // Lossless: a `usize` is at most 64 bits.
        if u64::from(removed) > count as u64 {
            return Err(ProtocolError::field_value(
                command,
                "removed",
                removed.into(),
            ));
        }
        Ok(Self {
            tenant: header.u64_at(0),
            epoch,
            removed,
            terms,
        })
    }

    fn to_body(&self) -> Result<Purge, ProtocolError> {
        let iris = (0..self.iri_count())
            .filter_map(|index| self.iri(index))
            .map(str::to_string)
            .collect();
        Ok(Purge::new(self.tenant, iris).purged(self.epoch, self.removed))
    }
}
