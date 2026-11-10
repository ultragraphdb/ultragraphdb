/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::catalog_announcement::CatalogAnnouncement;
use crate::types::command::Command;
use crate::types::epoch_close::EpochClose;
use crate::types::epoch_close_view::EpochCloseView;
use crate::types::frame_view::FrameView;
use crate::utils::body::read_sections;
use crate::utils::command_header::expect_command;
use crate::utils::fields::{boolean, epoch};
use crate::utils::whole_records::whole_records;

impl<'a> BodyView<'a> for EpochCloseView<'a> {
    const COMMAND: Command = Command::EpochClose;

    type Body = EpochClose;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 17), (24, 32)])?;
        let closed = epoch(command, "epoch", header.u64_at(0), false)?;
        let more = boolean(command, "more", header.u8_at(16))?;
        let sections = read_sections::<2>(command, frame.body(), 0)?;
        let announcements =
            whole_records::<{ CatalogAnnouncement::SIZE }>(command, 0, sections.get(0))?;
        let evictions = whole_records::<16>(command, 1, sections.get(1))?;
        let mut previous = (0u64, 0u64);
        for (i, bytes) in announcements.iter().enumerate() {
            let announcement = CatalogAnnouncement::decode(bytes);
            let key = (announcement.tenant(), announcement.version());
            if announcement.version() == 0 || key <= previous {
                return Err(ProtocolError::field_value(
                    command,
                    "announcement",
                    i as u64, // lossless: a `usize` is at most 64 bits
                ));
            }
            previous = key;
        }
        let mut before = [0u8; 16];
        for (i, session) in evictions.iter().enumerate() {
            if *session <= before {
                // Lossless: a `usize` is at most 64 bits.
                return Err(ProtocolError::field_value(command, "eviction", i as u64));
            }
            before = *session;
        }
        Ok(Self {
            epoch: closed,
            time: header.u64_at(8),
            more,
            placement: header.u64_at(24),
            announcements: sections.get(0),
            evictions: sections.get(1),
        })
    }

    fn to_body(&self) -> Result<EpochClose, ProtocolError> {
        let mut close = EpochClose::new(self.epoch, self.time).with_placement(self.placement);
        if self.more {
            close = close.with_more();
        }
        (0..self.announcement_count())
            .filter_map(|i| self.announcement(i))
            .for_each(|a| close.push_announcement(a));
        (0..self.eviction_count())
            .filter_map(|i| self.eviction(i))
            .for_each(|s| close.push_eviction(s));
        Ok(close)
    }
}
