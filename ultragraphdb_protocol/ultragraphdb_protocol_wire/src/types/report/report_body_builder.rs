/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::report::Report;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Report {
    const COMMAND: Command = Command::Report;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.partition.to_le_bytes()),
            (4, &[self.kind.number()]),
            (8, &self.tenant.to_le_bytes()),
            (16, &self.version.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let (records, text) = self.terms.sections();
        write_sections(&[], &[&records, &text])
    }
}
