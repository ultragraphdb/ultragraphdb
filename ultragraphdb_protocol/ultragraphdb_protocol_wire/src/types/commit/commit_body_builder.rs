/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::commit::Commit;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::results::encode_results;
use crate::utils::sections::write_sections;

impl BodyBuilder for Commit {
    const COMMAND: Command = Command::Commit;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.session.to_le_bytes()),
            (8, &self.request.to_le_bytes()),
            (16, &self.commit_version.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[&encode_results(self.results())])
    }
}
