/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::entries::Entries;
use crate::types::entries_end::EntriesEnd;
use crate::types::entries_view::EntriesView;
use crate::types::frame_view::FrameView;
use crate::types::key_values::KeyValues;
use crate::utils::fields::epoch;
use crate::utils::sections::{Sections, expect_command};

impl<'a> BodyView<'a> for EntriesView<'a> {
    const COMMAND: Command = Command::Entries;

    type Body = Entries;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 9), (16, 24)])?;
        let number = header.u8_at(8);
        let end = EntriesEnd::from_number(number)
            .ok_or_else(|| ProtocolError::field_value(command, "end", number.into()))?;
        let read_at = epoch(command, "epoch", header.u64_at(16), true)?;
        let sections = Sections::<1>::read(command, frame.body(), 0)?;
        let entries = KeyValues::read(command, 0, sections.offset(0), sections.get(0))?;
        let mut previous: Option<&[u8]> = None;
        for (index, entry) in entries.iter().enumerate() {
            if entry.value().is_none() {
                return Err(ProtocolError::field_value(command, "entry", index as u64));
            }
            if previous.is_some_and(|key| entry.key() <= key) {
                return Err(ProtocolError::field_value(
                    command,
                    "entry order",
                    index as u64,
                ));
            }
            previous = Some(entry.key());
        }
        let count = header.u32_at(0);
        if entries.len() != count as usize {
            return Err(ProtocolError::count_mismatch(
                command,
                "entries",
                entries.len() as u64,
                count.into(),
            ));
        }
        Ok(Self {
            range: header.u32_at(4),
            end,
            epoch: read_at,
            entries,
        })
    }

    fn to_body(&self) -> Result<Entries, ProtocolError> {
        let mut entries = Entries::new(self.range, self.end, self.epoch);
        self.entries
            .iter()
            .for_each(|entry| entries.push(entry.key(), entry.value().unwrap_or_default()));
        Ok(entries)
    }
}
