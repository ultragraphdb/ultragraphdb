/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::default_entry::DefaultEntry;
use crate::types::term_table::TermTable;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for DefaultEntry {
    const COMMAND: Command = Command::DefaultEntry;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.namespace.to_le_bytes()),
            (8, &[u8::from(self.found)]),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut terms = TermTable::new();
        let _predicate: u32 = terms.iri(&self.predicate);
        let (records, text) = terms.sections();
        write_sections(&[], &[&records, &text])
    }
}
