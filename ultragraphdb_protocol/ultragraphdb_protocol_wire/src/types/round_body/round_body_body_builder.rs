/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::decision::Decision;
use crate::types::round_body::RoundBody;
use crate::utils::constants::ABSENT;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;
use crate::utils::sections::{align8, as_u32};

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
                        committed += 1;
                        (0u16, committed - 1, 0u64)
                    }
                    Decision::Aborted { code, detail } => (code.number(), ABSENT, *detail),
                };
                outcome
                    .to_le_bytes()
                    .into_iter()
                    .chain([0, 0])
                    .chain(record.to_le_bytes())
                    .chain(detail.to_le_bytes())
                    .collect::<Vec<u8>>()
            })
            .collect();
        let mut table = Vec::new();
        let mut records = Vec::new();
        for record in self.records() {
            records.resize(align8(records.len()), 0);
            table.extend_from_slice(&as_u32(records.len()).to_le_bytes());
            table.extend_from_slice(&as_u32(record.len()).to_le_bytes());
            records.extend_from_slice(record);
        }
        write_sections(&[], &[&decisions, &table, &records])
    }
}
