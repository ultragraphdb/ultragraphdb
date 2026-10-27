/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::column_type::ColumnType;
use crate::types::command::Command;
use crate::types::prepare::Prepare;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Prepare {
    const COMMAND: Command = Command::Prepare;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.namespace().to_le_bytes()),
            (8, &self.statement().to_le_bytes()),
            (16, &self.schema().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let (records, text) = self.terms().sections();
        let types = |types: &[ColumnType]| types.iter().map(|t| t.number()).collect::<Vec<u8>>();
        write_sections(
            &[],
            &[
                &self.pattern().write(),
                &records,
                &text,
                &types(self.parameter_types()),
                &types(self.result_types()),
            ],
        )
    }
}
