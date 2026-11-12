/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::language::Language;
use crate::types::query::Query;
use crate::types::query_view::QueryView;
use crate::types::term_table_view::TermTableView;
use crate::utils::body::{read_sections, read_string_list, text, u32s};
use crate::utils::command_header::expect_command;
use crate::utils::fields::count;

impl<'a> BodyView<'a> for QueryView<'a> {
    const COMMAND: Command = Command::Query;

    type Body = Query;

    /// Checks, in `PROTOCOL.md` §7.4's order: unused bytes 9–11 and 16–127 zero; `language`, 1 or 2; the sections; the
    /// term table; the names; the values a whole number of u32s, one per name, each below the term table's length; the
    /// text UTF-8. A receiver holds the text to its `max_frame` by [`QueryView::check_text`].
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 9), (12, 16)])?;
        let number = header.u8_at(8);
        let language = Language::from_number(number)
            .filter(|language| *language != Language::Pattern)
            .ok_or_else(|| ProtocolError::field_value(command, "language", number.into()))?;
        let sections = read_sections::<7>(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (4, 5),
            sections.offset(4),
            sections.get(4),
            sections.get(5),
        )?;
        let names = read_string_list(command, (1, 2), sections.get(1), sections.get(2))?;
        let values = u32s(command, 3, sections.get(3))?;
        if values.len() != names.len() {
            return Err(ProtocolError::count_mismatch(
                command,
                "parameters",
                // The casts are lossless: a `usize` is at most 64 bits.
                values.len() as u64,
                names.len() as u64,
            ));
        }
        let limit = count(command, "terms", terms.len())?;
        if let Some((at, value)) = values.iter().enumerate().find(|(_, v)| **v >= limit) {
            return Err(ProtocolError::index_out_of_range(
                command, 3, at, *value, limit,
            ));
        }
        Ok(Self {
            transaction: header.u64_at(0),
            language,
            limit: header.u32_at(12),
            text: text(command, 0, sections.get(0))?,
            names,
            values: sections.get(3),
            terms,
            resume: sections.get(6),
        })
    }

    fn to_body(&self) -> Result<Query, ProtocolError> {
        let query = Query::new(
            self.transaction,
            self.language,
            self.text,
            self.terms.to_table(),
        )
        .with_limit(self.limit)
        .with_resume(self.resume.to_vec());
        Ok((0..self.parameter_count())
            .filter_map(|index| self.parameter(index))
            .fold(query, |query, (name, value)| {
                query.with_parameter(name, value)
            }))
    }
}
