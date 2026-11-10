/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{Sections, StringList, as_u32, u32_at};
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::monotonic::Monotonic;
use crate::types::result_batch::ResultBatch;
use crate::types::result_term::ResultTerm;
use crate::types::result_term_view::{named_hash, result_encoding_whole};
use crate::types::result_view::ResultView;
use crate::types::warnings::Warnings;
use crate::utils::body::{read_sections, read_string_list, records, u32s};
use crate::utils::command_header::expect_command;
use crate::utils::constants::{ABSENT, FLAG_LAST, QUERY_COLUMNS_MAX};
use crate::utils::fields;

impl<'a> BodyView<'a> for ResultView<'a> {
    const COMMAND: Command = Command::Result;

    type Body = ResultBatch;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 21)])?;
        let (columns, rows) = (header.u32_at(0), header.u32_at(4));
        // A GQL statement without `RETURN` answers no column and no row.
        if columns > QUERY_COLUMNS_MAX || (columns == 0 && rows != 0) {
            return Err(ProtocolError::field_value(
                command,
                "columns",
                columns.into(),
            ));
        }
        let epoch = fields::epoch(command, "epoch", header.u64_at(8), true)?;
        let number = header.u8_at(20);
        let monotonic = Monotonic::from_number(number)
            .ok_or_else(|| ProtocolError::field_value(command, "monotonic", number.into()))?;
        let sections = read_sections::<12>(command, frame.body(), 0)?;
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
        let (strings, column_names, warnings) = read_lists(command, &sections, columns)?;
        check_cells(
            command,
            sections.get(0),
            u64::from(columns) * u64::from(rows),
            as_u32(terms),
        )?;
        let encodings = sections.get(2);
        check_encodings(command, &offsets, encodings)?;
        check_texts(command, texts, as_u32(strings.len()))?;
        check_term_texts(command, texts, &offsets, encodings)?;
        let resume = sections.get(6);
        if !resume.is_empty() && frame.header().flags() & FLAG_LAST == 0 {
            return Err(ProtocolError::field_value(
                command,
                "resume key",
                resume.len() as u64, // lossless: a `usize` is at most 64 bits
            ));
        }
        Ok(Self {
            columns,
            rows,
            cells: sections.get(0),
            offsets,
            encodings,
            texts,
            strings,
            resume,
            epoch,
            partition: header.u32_at(16),
            monotonic,
            column_names,
            warnings,
        })
    }

    fn to_body(&self) -> Result<ResultBatch, ProtocolError> {
        let command = Self::COMMAND.number();
        let terms = (0..self.term_count() as u32)
            .map(|i| {
                let view = self
                    .term(i)
                    .ok_or(ProtocolError::bad_term(command, 2, i as usize))?; // lossless: a `usize` is at least 32 bits
                if let Some((hash, blank_node)) = named_hash(view.encoding()) {
                    return Ok(ResultTerm::named(hash, blank_node, view.text()));
                }
                let term = view
                    .term()
                    .ok_or(ProtocolError::bad_term(command, 2, i as usize))?; // lossless: a `usize` is at least 32 bits
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
            .with_read_at(self.epoch, self.partition)
            .with_monotonic(self.monotonic)
            .with_column_names(self.column_names.iter().map(str::to_string).collect())
            .with_warnings(self.warnings.to_warnings());
        for row in 0..self.rows {
            let cells: Vec<Option<&ResultTerm>> = (0..self.columns)
                .map(|column| self.cell(column, row).and_then(|i| terms.get(i as usize)))
                .collect();
            batch.push_query_row(&cells)?;
        }
        Ok(batch)
    }
}

/// The batch's string lists, checked in §9.1's order: the strings (4, 5); the column names (7, 8), empty or one per
/// column ([`ProtocolError::count_mismatch`] `"column names"`); the warnings (9 to 11, §4.9), each of source 0.
fn read_lists<'a>(
    command: u16,
    sections: &Sections<'a, 12>,
    columns: u32,
) -> Result<(StringList<'a>, StringList<'a>, Warnings<'a>), ProtocolError> {
    let strings = read_string_list(command, (4, 5), sections.get(4), sections.get(5))?;
    let column_names = read_string_list(command, (7, 8), sections.get(7), sections.get(8))?;
    // Lossless: a `usize` is at least 32 bits.
    if !column_names.is_empty() && column_names.len() != columns as usize {
        return Err(ProtocolError::count_mismatch(
            command,
            "column names",
            column_names.len() as u64, // lossless: a `usize` is at most 64 bits
            columns.into(),
        ));
    }
    let warnings = Warnings::read(
        command,
        (9, 10, 11),
        sections.offset(9),
        (sections.get(9), sections.get(10), sections.get(11)),
        false,
    )?;
    Ok((strings, column_names, warnings))
}

/// Whether a term of `tag` names a `text` and an `annotation` (`PROTOCOL.md` §10.3).
fn names_text(tag: u8) -> (bool, bool) {
    match tag {
        0x01 | 0x02 | 0x21 | 0x23 | 0x26 | 0x29 | 0x31 => (true, false),
        0x24 | 0x27 | 0x2A | 0x33 => (true, true),
        0x32 => (false, true),
        _ => (false, false),
    }
}

/// Refused as [`ProtocolError::field_value`] (`"term text"`, the term's index) unless each term's `text` and `annotation`
/// is a string index where §10.3's table names one and [`ABSENT`] elsewhere: the offsets, encodings and text indices
/// already checked.
fn check_term_texts(
    command: u16,
    texts: &[u8],
    offsets: &[u32],
    encodings: &[u8],
) -> Result<(), ProtocolError> {
    for (index, start) in offsets.iter().enumerate() {
        // Lossless: a `usize` is at least 32 bits.
        let tag = encodings.get(*start as usize).copied().unwrap_or_default();
        let (text, annotation) = names_text(tag);
        let held = |at: usize| u32_at(texts, at).is_some_and(|string| string != ABSENT);
        if held(2 * index) != text || held(2 * index + 1) != annotation {
            return Err(ProtocolError::field_value(
                command,
                "term text",
                index as u64, // lossless: a `usize` is at most 64 bits
            ));
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
    // Lossless: a `usize` is at most 64 bits.
    if cells.len() as u64 != expected {
        return Err(ProtocolError::count_mismatch(
            command,
            "cells",
            cells.len() as u64, // lossless: a `usize` is at most 64 bits
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
            encodings.len() as u64, // lossless: a `usize` is at most 64 bits
            0,
        ));
    }
    for (at, start) in offsets.iter().enumerate() {
        let start = *start as usize; // lossless: a `usize` is at least 32 bits
        let end = offsets
            .get(at + 1)
            .map_or(encodings.len(), |end| *end as usize); // lossless: a `usize` is at least 32 bits
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
