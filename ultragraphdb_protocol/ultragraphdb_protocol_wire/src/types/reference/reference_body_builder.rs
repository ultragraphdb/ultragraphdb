/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::reference::Reference;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Reference {
    const COMMAND: Command = Command::Reference;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &[u8::from(self.reply)]),
            (8, &self.namespace.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let registrations: Vec<u8> = self
            .registrations
            .iter()
            .flat_map(|registration| registration.encode())
            .collect();
        let (records, text) = self.terms.sections();
        write_sections(&[], &[&registrations, &records, &text])
    }
}
