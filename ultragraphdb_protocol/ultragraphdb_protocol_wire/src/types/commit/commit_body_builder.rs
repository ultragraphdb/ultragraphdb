/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::commit::Commit;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::results::encode_results;
use crate::utils::token;

impl BodyBuilder for Commit {
    const COMMAND: Command = Command::Commit;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.session),
            (16, &self.request.to_le_bytes()),
            (24, &self.commit_version.to_le_bytes()),
            (32, &token::encode(self.token())),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let (records, text) = self.nodes().sections();
        write_sections(&[], &[&encode_results(self.results()), &records, &text])
    }
}
