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
use crate::types::result_view::ResultView;
use crate::types::string_list::StringList;
use crate::utils::constants::ABSENT;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::{as_u32, records, u32s};
use ultragraphdb_core_terms::EncodedTerm;

impl<'a> BodyView<'a> for ResultView<'a> {
    const COMMAND: Command = Command::Result;

    type Body = ResultBatch;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 8)])?;
        let (columns, rows) = (header.u32_at(0), header.u32_at(4));
        let sections = Sections::<7>::read(command, frame.body(), 0)?;
        let offsets = u32s(command, 1, sections.get(1))?;
        let terms = offsets.len();
        let texts = sections.get(3);
        let text_count = records(command, 3, texts, 8)?.len();
        if text_count != terms {
            return Err(ProtocolError::CountMismatch(
                command,
                "term texts",
                text_count as u64,
                terms as u64,
            ));
        }
        let strings = StringList::read(command, (4, 5), sections.get(4), sections.get(5))?;
        let cells = u32s(command, 0, sections.get(0))?;
        let expected = u64::from(columns) * u64::from(rows);
        if cells.len() as u64 != expected {
            return Err(ProtocolError::CountMismatch(
                command,
                "cells",
                cells.len() as u64,
                expected,
            ));
        }
        let limit = as_u32(terms);
        if let Some((at, cell)) = cells
            .iter()
            .enumerate()
            .find(|(_, cell)| **cell != ABSENT && **cell >= limit)
        {
            return Err(ProtocolError::IndexOutOfRange(command, 0, at, *cell, limit));
        }
        let encodings = sections.get(2);
        if terms == 0 && !encodings.is_empty() {
            return Err(ProtocolError::CountMismatch(
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
            let whole = (at > 0 || start == 0)
                && encodings
                    .get(start..end)
                    .is_some_and(|bytes| EncodedTerm::from_bytes(bytes).is_ok());
            if !whole {
                return Err(ProtocolError::BadTerm(command, 2, at));
            }
        }
        let string_limit = as_u32(strings.len());
        let texts_read = u32s(command, 3, texts)?;
        if let Some((at, index)) = texts_read
            .iter()
            .enumerate()
            .find(|(_, i)| **i != ABSENT && **i >= string_limit)
        {
            return Err(ProtocolError::IndexOutOfRange(
                command,
                3,
                at,
                *index,
                string_limit,
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
            resume: sections.get(6),
        })
    }

    fn to_body(&self) -> Result<ResultBatch, ProtocolError> {
        let command = Self::COMMAND.number();
        let terms = (0..self.term_count() as u32)
            .map(|i| {
                let view = self
                    .term(i)
                    .ok_or(ProtocolError::BadTerm(command, 2, i as usize))?;
                let term = view
                    .term()
                    .ok_or(ProtocolError::BadTerm(command, 2, i as usize))?;
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
        let mut batch = ResultBatch::new(self.columns).with_resume(self.resume.to_vec());
        for row in 0..self.rows {
            let cells: Vec<Option<&ResultTerm>> = (0..self.columns)
                .map(|column| self.cell(column, row).and_then(|i| terms.get(i as usize)))
                .collect();
            batch.push_row(&cells)?;
        }
        Ok(batch)
    }
}
