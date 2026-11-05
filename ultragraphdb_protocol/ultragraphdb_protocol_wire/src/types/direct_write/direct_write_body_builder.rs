/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::direct_write::DirectWrite;
use crate::utils::changes::ChangeWriter;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::{u32_bytes, write_sections};

impl BodyBuilder for DirectWrite {
    const COMMAND: Command = Command::DirectWrite;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        self.header().encode()
    }

    /// The sections, the names always empty: a change given as a declaration is written as a record alone, which the
    /// view refuses.
    fn body(&self) -> Vec<u8> {
        let mut writer = ChangeWriter::default();
        self.changes()
            .iter()
            .for_each(|change| writer.change(change));
        let (records, text) = self.terms().sections();
        write_sections(
            &[],
            &[
                &writer.records,
                &u32_bytes(&writer.args),
                &records,
                &text,
                &[],
                &[],
            ],
        )
    }
}
