/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{u32_bytes, write_sections};

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::join::Join;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Join {
    const COMMAND: Command = Command::Join;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[(0, &[self.kind.number()])])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(
            &[],
            &[self.address.as_bytes(), &u32_bytes(&self.partitions)],
        )
    }
}
