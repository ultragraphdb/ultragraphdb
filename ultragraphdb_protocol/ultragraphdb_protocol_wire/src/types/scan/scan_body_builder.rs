/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::scan::Scan;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::constants::SCAN_RANGE_SIZE;
use crate::utils::sections::as_u32;
use crate::utils::sections::write_sections;

impl BodyBuilder for Scan {
    const COMMAND: Command = Command::Scan;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &[self.at().number()]),
            (8, &self.read_version().to_le_bytes()),
            (16, &self.epoch().to_le_bytes()),
            (24, &self.principal().to_le_bytes()),
            (32, &self.transaction()),
            (48, &self.limit().to_le_bytes()),
            (52, &self.deadline_ms().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut records = Vec::with_capacity(SCAN_RANGE_SIZE.saturating_mul(self.ranges().count()));
        let mut keys = Vec::new();
        for range in self.ranges() {
            records.extend_from_slice(&range.encode(as_u32(keys.len())));
            keys.extend_from_slice(range.first());
            keys.extend_from_slice(range.past());
            keys.extend_from_slice(range.after());
        }
        let (term_records, text) = self.terms().sections();
        write_sections(&[], &[&records, &keys, &term_records, &text])
    }
}
