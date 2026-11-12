/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{align8, as_u32, write_sections};

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::decision::Decision;
use crate::types::round_body::{RECORD_ENTRY_SIZE, RoundBody};
use crate::utils::constants::{ABSENT, COMMAND_HEADER_SIZE};
use crate::utils::key_value_writer::write_entries;

impl BodyBuilder for RoundBody {
    const COMMAND: Command = Command::Round;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        self.header().encode()
    }

    fn body(&self) -> Vec<u8> {
        let mut committed = 0u32;
        let decisions: Vec<u8> = self
            .decisions()
            .iter()
            .flat_map(|decision| {
                let (outcome, record, detail) = match decision {
                    Decision::Committed => {
                        committed = committed.saturating_add(1);
                        (0u16, committed.saturating_sub(1), 0u64)
                    }
                    Decision::Aborted { code, detail } => (code.number(), ABSENT, *detail),
                };
                outcome
                    .to_le_bytes()
                    .into_iter()
                    .chain([0, 0])
                    .chain(record.to_le_bytes())
                    .chain(detail.to_le_bytes())
            })
            .collect();
        let mut table = Vec::with_capacity(RECORD_ENTRY_SIZE.saturating_mul(self.records().len()));
        let mut records = Vec::new();
        for record in self.records() {
            records.resize(align8(records.len()), 0);
            table.extend_from_slice(&as_u32(records.len()).to_le_bytes());
            table.extend_from_slice(&as_u32(record.len()).to_le_bytes());
            records.extend_from_slice(record);
        }
        let dictionary = write_entries(
            self.dictionary()
                .iter()
                .map(|(key, value)| (key.as_slice(), Some(value.as_slice()))),
        );
        write_sections(&[], &[&decisions, &table, &records, &dictionary])
    }
}
