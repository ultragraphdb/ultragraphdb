/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::drop_command::DropCommand;
use crate::types::term_table::TermTable;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for DropCommand {
    const COMMAND: Command = Command::Drop;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &[self.kind().number()]),
            (8, &self.namespace().to_le_bytes()),
            (16, &self.session().to_le_bytes()),
            (24, &self.request().to_le_bytes()),
            (32, &self.commit_version().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut terms = TermTable::new();
        if let Some(target) = self.target() {
            terms.iri(target);
        }
        let (records, text) = terms.sections();
        write_sections(&[], &[&records, &text])
    }
}
