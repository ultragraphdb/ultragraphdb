/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::StringList;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::install::Install;
use crate::types::install_view::InstallView;
use crate::types::warnings::Warnings;
use crate::utils::body::{read_sections, read_string_list, text};
use crate::utils::command_header::expect_command;
use crate::utils::constants::SOURCE_NAME_MAX;
use crate::utils::fields::{at_least_one, epoch};

impl<'a> BodyView<'a> for InstallView<'a> {
    const COMMAND: Command = Command::Install;

    type Body = Install;

    /// Checks, in `PROTOCOL.md` §8.4's order: unused bytes 32–127 zero; `version`, in a reply at least 1; `epoch`, 0 in a
    /// request and an epoch in a reply, a body of `epoch` 0 being a request; `staged`, 0 in a request; the seven
    /// sections; in a request, sections 2 to 6 empty; the string list, an even count of strings and at least two, each
    /// name at most [`SOURCE_NAME_MAX`] bytes; the reports UTF-8; the migration report empty when `staged` is 0; the
    /// warnings (§4.9).
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 32)])?;
        let takes_effect = header.u64_at(16);
        let reply = takes_effect != 0;
        let version = header.u64_at(8);
        if reply {
            let _installed: u64 = at_least_one(command, "version", version)?;
        }
        let takes_effect = epoch(command, "epoch", takes_effect, true)?;
        let staged = header.u64_at(24);
        if !reply && staged != 0 {
            return Err(ProtocolError::field_value(command, "staged", staged));
        }
        let sections = read_sections::<7>(command, frame.body(), 0)?;
        if !reply && let Some(held) = (2..7).map(|i| sections.get(i)).find(|s| !s.is_empty()) {
            return Err(ProtocolError::count_mismatch(
                command,
                "report",
                held.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        }
        let sources = read_sources(command, sections.get(0), sections.get(1))?;
        let report = text(command, 2, sections.get(2))?;
        let migration_report = text(command, 3, sections.get(3))?;
        if staged == 0 && !migration_report.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "migration report",
                migration_report.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        }
        let warnings = Warnings::read(
            command,
            (4, 5, 6),
            sections.offset(4),
            (sections.get(4), sections.get(5), sections.get(6)),
            true,
        )?;
        Ok(Self {
            tenant: header.u64_at(0),
            version,
            epoch: takes_effect,
            sources,
            report,
            staged,
            migration_report,
            warnings,
        })
    }

    fn to_body(&self) -> Result<Install, ProtocolError> {
        let sources = (0..self.source_count())
            .filter_map(|index| self.source(index))
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect();
        Ok(Install::new(self.tenant, self.version, sources)
            .installed(self.version, self.epoch, self.report)
            .with_staged(self.staged)
            .with_migration_report(self.migration_report)
            .with_warnings(self.warnings.to_warnings()))
    }
}

/// The sources' string list of sections 0 and 1 (§4.3), an even count of strings and at least two
/// ([`ProtocolError::count_mismatch`] `"sources"`), each name at most [`SOURCE_NAME_MAX`] bytes
/// ([`ProtocolError::field_value`] `"source name"`).
fn read_sources<'a>(
    command: u16,
    ends: &'a [u8],
    text: &'a [u8],
) -> Result<StringList<'a>, ProtocolError> {
    let sources = read_string_list(command, (0, 1), ends, text)?;
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
    Ok(sources)
}
