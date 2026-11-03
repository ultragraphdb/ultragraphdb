/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::verify::Verify;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Verify {
    const COMMAND: Command = Command::Verify;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &[u8::from(self.verified())]),
            (8, &self.principal().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let signature = self.signature().map_or(&[][..], |s| &s[..]);
        write_sections(
            &[],
            &[
                self.name().as_bytes(),
                self.auth_message().as_bytes(),
                self.proof(),
                signature,
            ],
        )
    }
}
