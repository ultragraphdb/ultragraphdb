/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{u32_bytes, write_sections};

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::operation::Operation;
use crate::utils::command_header::command_header;
use crate::utils::commit_operation_layout::{OperationFields, push_args};
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Operation {
    const COMMAND: Command = Command::Operation;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[(0, &OperationFields::of(self.operation()).encode())])
    }

    fn body(&self) -> Vec<u8> {
        let mut args = Vec::new();
        push_args(self.operation(), &mut args);
        let (records, text) = self.terms().sections();
        write_sections(&[], &[&u32_bytes(&args), &records, &text])
    }
}
