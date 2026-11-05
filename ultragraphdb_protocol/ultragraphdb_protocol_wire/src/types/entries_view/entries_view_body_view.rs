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
use crate::utils::fields::{boolean, epoch};
use crate::utils::sections::{Sections, expect_command};

impl<'a> BodyView<'a> for EntriesView<'a> {
    const COMMAND: Command = Command::Entries;

    type Body = Entries;

    /// Checks, in `PROTOCOL.md` §13.14's order: unused bytes 10–15 and 24–127 zero; `end`; `continues` 0 or 1, 0 when `end`
    /// is not 0; `epoch`; the sections; each entry; every value present; each key above the one before; the count; at
    /// least one entry when `continues` is 1.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 10), (16, 24)])?;
        let number = header.u8_at(8);
        let end = EntriesEnd::from_number(number)
            .ok_or_else(|| ProtocolError::field_value(command, "end", number.into()))?;
        let continues = boolean(command, "continues", header.u8_at(9))?;
        if continues && end != EntriesEnd::More {
            return Err(ProtocolError::field_value(command, "continues", 1));
        }
        let read_at = epoch(command, "epoch", header.u64_at(16), true)?;
        let sections = Sections::<1>::read(command, frame.body(), 0)?;
        let entries = KeyValues::read(command, 0, sections.offset(0), sections.get(0))?;
        if let Some(index) = entries.iter().position(|entry| entry.value().is_none()) {
            return Err(ProtocolError::field_value(command, "entry", index as u64));
        }
        let mut previous: Option<&[u8]> = None;
        for (index, entry) in entries.iter().enumerate() {
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
        if continues && entries.is_empty() {
            return Err(ProtocolError::field_value(command, "continues", 1));
        }
        Ok(Self {
            range: header.u32_at(4),
            end,
            continues,
            epoch: read_at,
            entries,
        })
    }

    fn to_body(&self) -> Result<Entries, ProtocolError> {
        let mut entries = if self.continues {
            Entries::continuing(self.range, self.epoch)
        } else {
            Entries::new(self.range, self.end, self.epoch)
        };
        self.entries
            .iter()
            .for_each(|entry| entries.push(entry.key(), entry.value().unwrap_or_default()));
        Ok(entries)
    }
}
