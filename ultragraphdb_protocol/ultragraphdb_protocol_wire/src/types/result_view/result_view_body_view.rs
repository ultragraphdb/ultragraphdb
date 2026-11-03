/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::result_batch::ResultBatch;
use crate::types::result_term::ResultTerm;
use crate::types::result_term_view::{named_hash, result_encoding_whole};
use crate::types::result_view::ResultView;
use crate::types::string_list::StringList;
use crate::utils::constants::ABSENT;
use crate::utils::fields;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::{as_u32, records, u32_at, u32s};

impl<'a> BodyView<'a> for ResultView<'a> {
    const COMMAND: Command = Command::Result;

    type Body = ResultBatch;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 20)])?;
        let (columns, rows) = (header.u32_at(0), header.u32_at(4));
        if !(1..=5).contains(&columns) {
            return Err(ProtocolError::field_value(
                command,
                "columns",
                columns.into(),
            ));
        }
        let epoch = fields::epoch(command, "epoch", header.u64_at(8), true)?;
        let sections = Sections::<7>::read(command, frame.body(), 0)?;
        let offsets = u32s(command, 1, sections.get(1))?;
        let terms = offsets.len();
        let texts = sections.get(3);
        let text_count = records(command, 3, texts, 8)?.len();
        if text_count != terms {
            return Err(ProtocolError::count_mismatch(
                command,
                "term texts",
                text_count as u64,
                terms as u64,
            ));
        }
        let strings = StringList::read(command, (4, 5), sections.get(4), sections.get(5))?;
        check_cells(
            command,
            sections.get(0),
            u64::from(columns) * u64::from(rows),
            as_u32(terms),
        )?;
        let encodings = sections.get(2);
        check_encodings(command, &offsets, encodings)?;
        check_texts(command, texts, as_u32(strings.len()))?;
        check_reifiers(command, sections.get(0), &offsets, encodings, columns, rows)?;
        Ok(Self {
            columns,
            rows,
            cells: sections.get(0),
            offsets,
            encodings,
            texts,
            strings,
            resume: sections.get(6),
            epoch,
            partition: header.u32_at(16),
        })
    }

    fn to_body(&self) -> Result<ResultBatch, ProtocolError> {
        let command = Self::COMMAND.number();
        let terms = (0..self.term_count() as u32)
            .map(|i| {
                let view = self
                    .term(i)
                    .ok_or(ProtocolError::bad_term(command, 2, i as usize))?;
                if let Some((hash, blank_node)) = named_hash(view.encoding()) {
                    return Ok(ResultTerm::named(hash, blank_node, view.text()));
                }
                let term = view
                    .term()
                    .ok_or(ProtocolError::bad_term(command, 2, i as usize))?;
                let mut owned = ResultTerm::new(term);
                if let Some(text) = view.text() {
                    owned = owned.with_text(text);
                }
                if let Some(annotation) = view.annotation() {
                    owned = owned.with_annotation(annotation);
                }
                Ok(owned)
            })
            .collect::<Result<Vec<_>, ProtocolError>>()?;
        let mut batch = ResultBatch::new(self.columns)
            .with_resume(self.resume.to_vec())
            .with_read_at(self.epoch, self.partition);
        for row in 0..self.rows {
            let cells: Vec<Option<&ResultTerm>> = (0..self.columns)
                .map(|column| self.cell(column, row).and_then(|i| terms.get(i as usize)))
                .collect();
            batch.push_row(&cells)?;
        }
        Ok(batch)
    }
}

/// Refused as [`ProtocolError::field_value`] (`"reifier"`, the row) unless each cell of the last column, the reifier
/// column, is [`ABSENT`] or a term of tag `0x02`, a blank node's: the cells, offsets and encodings already checked.
fn check_reifiers(
    command: u16,
    cells: &[u8],
    offsets: &[u32],
    encodings: &[u8],
    columns: u32,
    rows: u32,
) -> Result<(), ProtocolError> {
    let first = u64::from(columns.saturating_sub(1)) * u64::from(rows);
    for row in 0..rows {
        let cell = usize::try_from(first + u64::from(row))
            .ok()
            .and_then(|at| u32_at(cells, at))
            .unwrap_or(ABSENT);
        if cell == ABSENT {
            continue;
        }
        let tag = offsets
            .get(cell as usize)
            .and_then(|start| encodings.get(*start as usize))
            .copied();
        if tag != Some(0x02) {
            return Err(ProtocolError::field_value(command, "reifier", row.into()));
        }
    }
    Ok(())
}

/// Refused unless the cells number `expected` and each is absent or below `limit`, the term count.
fn check_cells(
    command: u16,
    section: &[u8],
    expected: u64,
    limit: u32,
) -> Result<(), ProtocolError> {
    let cells = u32s(command, 0, section)?;
    if cells.len() as u64 != expected {
        return Err(ProtocolError::count_mismatch(
            command,
            "cells",
            cells.len() as u64,
            expected,
        ));
    }
    if let Some((at, cell)) = cells
        .iter()
        .enumerate()
        .find(|(_, cell)| **cell != ABSENT && **cell >= limit)
    {
        return Err(ProtocolError::index_out_of_range(
            command, 0, at, *cell, limit,
        ));
    }
    Ok(())
}

/// Refused unless every term's encoding, from its offset to the next, is one whole encoding, the first at 0.
fn check_encodings(command: u16, offsets: &[u32], encodings: &[u8]) -> Result<(), ProtocolError> {
    if offsets.is_empty() && !encodings.is_empty() {
        return Err(ProtocolError::count_mismatch(
            command,
            "term encodings",
            encodings.len() as u64,
            0,
        ));
    }
    for (at, start) in offsets.iter().enumerate() {
        let start = *start as usize;
        let end = offsets
            .get(at + 1)
            .map_or(encodings.len(), |end| *end as usize);
        let whole =
            (at > 0 || start == 0) && encodings.get(start..end).is_some_and(result_encoding_whole);
        if !whole {
            return Err(ProtocolError::bad_term(command, 2, at));
        }
    }
    Ok(())
}

/// Refused unless every term text is absent or below `string_limit`, the string count.
fn check_texts(command: u16, texts: &[u8], string_limit: u32) -> Result<(), ProtocolError> {
    let texts_read = u32s(command, 3, texts)?;
    if let Some((at, index)) = texts_read
        .iter()
        .enumerate()
        .find(|(_, i)| **i != ABSENT && **i >= string_limit)
    {
        return Err(ProtocolError::index_out_of_range(
            command,
            3,
            at,
            *index,
            string_limit,
        ));
    }
    Ok(())
}
