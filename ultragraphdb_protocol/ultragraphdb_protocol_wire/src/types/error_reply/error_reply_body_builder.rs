/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::error_reply::ErrorReply;
use crate::types::term_table::TermTable;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for ErrorReply {
    const COMMAND: Command = Command::Error;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.code.number().to_le_bytes()),
            (8, &self.detail.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut terms = TermTable::new();
        if let Some(value) = &self.value {
            // A value the table refuses is no value; ErrorReply::with_value states which terms a table holds.
            let _ = terms.intern(value.clone());
        }
        let (records, text) = terms.sections();
        write_sections(&[], &[self.message.as_bytes(), &records, &text])
    }
}
