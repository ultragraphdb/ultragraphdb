/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{StringWriter, write_sections};

use crate::traits::body_builder::BodyBuilder;
use crate::types::column_type::ColumnType;
use crate::types::command::Command;
use crate::types::prepare::Prepare;
use crate::types::warnings::warning_sections;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Prepare {
    const COMMAND: Command = Command::Prepare;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.tenant().to_le_bytes()),
            (8, &self.statement().to_le_bytes()),
            (16, &self.schema().to_le_bytes()),
            (24, &[self.language().number()]),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let (records, term_text) = self.terms().sections();
        let types = |types: &[ColumnType]| types.iter().map(|t| t.number()).collect::<Vec<u8>>();
        let source = match (self.pattern(), self.text()) {
            (Some(pattern), _) => pattern.write().to_vec(),
            (None, text) => text.unwrap_or_default().as_bytes().to_vec(),
        };
        let mut names = StringWriter::default();
        for name in self.names() {
            let _index: u32 = names.push(name);
        }
        let (ends, names_text) = names.sections();
        let [warnings, message_ends, message_text] = warning_sections(self.warnings());
        write_sections(
            &[],
            &[
                &source,
                &records,
                &term_text,
                &types(self.parameter_types()),
                &types(self.result_types()),
                &ends,
                &names_text,
                &warnings,
                &message_ends,
                &message_text,
            ],
        )
    }
}
