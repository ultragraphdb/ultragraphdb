/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::purge::Purge;
use crate::types::term_table::TermTable;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Purge {
    const COMMAND: Command = Command::Purge;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.tenant().to_le_bytes()),
            (8, &self.epoch().to_le_bytes()),
            (16, &self.removed().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut terms = TermTable::new();
        for iri in self.iris() {
            let _index: u32 = terms.iri(iri);
        }
        let (records, text) = terms.sections();
        write_sections(&[], &[&records, &text])
    }
}
