/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{StringWriter, as_u32, u32_bytes, write_sections};

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::result_batch::ResultBatch;
use crate::types::warnings::warning_sections;
use crate::utils::command_header::command_header;
use crate::utils::constants::ABSENT;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for ResultBatch {
    const COMMAND: Command = Command::Result;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.columns().to_le_bytes()),
            (4, &self.rows().to_le_bytes()),
            (8, &self.epoch().to_le_bytes()),
            (16, &self.partition().to_le_bytes()),
            (20, &[self.monotonic().number()]),
        ])
    }

    fn body(&self) -> Vec<u8> {
        // Cells column by column.
        let cells: Vec<u32> = (0..self.columns())
            .flat_map(|column| (0..self.rows()).map(move |row| (column, row)))
            .map(|(column, row)| self.cell(column, row).unwrap_or(ABSENT))
            .collect();
        let mut offsets = Vec::with_capacity(self.terms().len());
        let mut encodings = Vec::new();
        let mut texts = Vec::with_capacity(2 * self.terms().len());
        let mut strings = StringWriter::default();
        for term in self.terms() {
            offsets.push(as_u32(encodings.len()));
            encodings.extend_from_slice(&term.encoding());
            texts.push(term.text().map_or(ABSENT, |t| strings.intern(t)));
            texts.push(term.annotation().map_or(ABSENT, |a| strings.intern(a)));
        }
        let (ends, text) = strings.sections();
        let mut names = StringWriter::default();
        for name in self.column_names() {
            let _index: u32 = names.push(name);
        }
        let (name_ends, name_text) = names.sections();
        let [warnings, message_ends, message_text] = warning_sections(self.warnings());
        write_sections(
            &[],
            &[
                &u32_bytes(&cells),
                &u32_bytes(&offsets),
                &encodings,
                &u32_bytes(&texts),
                &ends,
                &text,
                self.resume(),
                &name_ends,
                &name_text,
                &warnings,
                &message_ends,
                &message_text,
            ],
        )
    }
}
