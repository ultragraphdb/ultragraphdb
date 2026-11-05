/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::query::Query;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::{u32_bytes, write_sections};
use crate::utils::string_writer::StringWriter;

impl BodyBuilder for Query {
    const COMMAND: Command = Command::Query;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.transaction().to_le_bytes()),
            (8, &[self.language().number()]),
            (12, &self.limit().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut names = StringWriter::default();
        let values: Vec<u32> = self
            .parameters()
            .iter()
            .map(|(name, value)| {
                let _index: u32 = names.push(name);
                *value
            })
            .collect();
        let (ends, text) = names.sections();
        let (records, term_text) = self.terms().sections();
        write_sections(
            &[],
            &[
                self.text().as_bytes(),
                &ends,
                &text,
                &u32_bytes(&values),
                &records,
                &term_text,
                self.resume(),
            ],
        )
    }
}
