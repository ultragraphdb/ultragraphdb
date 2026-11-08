/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::install::Install;
use crate::types::install_view::InstallView;
use crate::types::string_list::StringList;
use crate::utils::constants::SOURCE_NAME_MAX;
use crate::utils::fields::{at_least_one, epoch};
use crate::utils::sections::{Sections, expect_command, text};

impl<'a> BodyView<'a> for InstallView<'a> {
    const COMMAND: Command = Command::Install;

    type Body = Install;

    /// Checks, in `PROTOCOL.md` §8.4's order: unused bytes 24–127 zero; `version`, in a reply at least 1; `epoch`, 0 in a
    /// request and an epoch in a reply, a body of `epoch` 0 being a request; the sections; in a request, no report; the
    /// string list, an even count of strings and at least two, each name at most [`SOURCE_NAME_MAX`] bytes; the report
    /// UTF-8.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 24)])?;
        let takes_effect = header.u64_at(16);
        let reply = takes_effect != 0;
        let version = header.u64_at(8);
        if reply {
            let _installed: u64 = at_least_one(command, "version", version)?;
        }
        let takes_effect = epoch(command, "epoch", takes_effect, true)?;
        let sections = Sections::<3>::read(command, frame.body(), 0)?;
        let report = sections.get(2);
        if !reply && !report.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "report",
                report.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        }
        let sources = StringList::read(command, (0, 1), sections.get(0), sections.get(1))?;
        let count = sources.len();
        if count < 2 || !count.is_multiple_of(2) {
            let expected = count.max(1).next_multiple_of(2);
            return Err(ProtocolError::count_mismatch(
                command,
                "sources",
                // The casts are lossless: a `usize` is at most 64 bits.
                count as u64,
                expected as u64,
            ));
        }
        if let Some(name) = sources
            .iter()
            .step_by(2)
            .find(|name| name.len() > SOURCE_NAME_MAX)
        {
            return Err(ProtocolError::field_value(
                command,
                "source name",
                name.len() as u64, // lossless: a `usize` is at most 64 bits
            ));
        }
        Ok(Self {
            tenant: header.u64_at(0),
            version,
            epoch: takes_effect,
            sources,
            report: text(command, 2, report)?,
        })
    }

    fn to_body(&self) -> Result<Install, ProtocolError> {
        let sources = (0..self.source_count())
            .filter_map(|index| self.source(index))
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect();
        Ok(Install::new(self.tenant, self.version, sources).installed(
            self.version,
            self.epoch,
            self.report,
        ))
    }
}
