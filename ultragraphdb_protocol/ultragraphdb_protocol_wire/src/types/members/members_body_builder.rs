/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::member::Member;
use crate::types::members::Members;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;
use crate::utils::string_writer::StringWriter;

impl BodyBuilder for Members {
    const COMMAND: Command = Command::Members;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        [0; COMMAND_HEADER_SIZE]
    }

    fn body(&self) -> Vec<u8> {
        let mut addresses = StringWriter::default();
        let members: Vec<u8> = self
            .members
            .iter()
            .flat_map(|(kind, partition, address)| {
                Member::new(*kind, *partition, addresses.intern(address)).encode()
            })
            .collect();
        let (ends, text) = addresses.sections();
        write_sections(&[], &[&members, &ends, &text])
    }
}
