/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::check::Check;
use crate::types::check_kind::CheckKind;
use crate::types::check_outcome::CheckOutcome;
use crate::types::check_view::{CHECK_SIZE, CheckRecord, CheckView, refused};
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::body::read_sections;
use crate::utils::command_header::{CommandHeader, expect_command};
use crate::utils::fields::ascends;
use crate::utils::whole_records::whole_records;

/// The offset within a check of its first padding byte.
const PADDING_AT: usize = 26;

/// Check `index` of a section at the body offset `section_at`, checked in the protocol's order: bytes 26–31 zero;
/// `session` not all zero and above `before`; `kind` 1 or 2; `outcome` 0, 1, 12, 14 or 21.
fn check_record(
    section_at: usize,
    index: usize,
    record: &[u8; CHECK_SIZE],
    before: Option<&[u8; 16]>,
) -> Result<[u8; 16], ProtocolError> {
    let padded = record.iter().skip(PADDING_AT).position(|byte| *byte != 0);
    if let Some(byte) = padded {
        // Within a body of at most 1 MiB, so the offset never saturates.
        let offset = section_at
            .saturating_add(CHECK_SIZE.saturating_mul(index))
            .saturating_add(PADDING_AT)
            .saturating_add(byte);
        return Err(ProtocolError::padding_not_zero(
            Command::Check.number(),
            offset,
        ));
    }
    let fields = CheckRecord::of(record);
    let held = ascends(&fields.session, before)
        && CheckKind::from_number(fields.kind).is_some()
        && CheckOutcome::from_number(fields.outcome).is_some();
    if held {
        Ok(fields.session)
    } else {
        Err(refused(index))
    }
}

impl<'a> CheckView<'a> {
    /// The body of `header` and `body`, checked as [`BodyView::from_frame`] states.
    pub(crate) fn read(header: CommandHeader<'a>, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Check.number();
        header.unused_zero(&[])?;
        let sections = read_sections::<1>(command, body, 0)?;
        let checks = whole_records::<CHECK_SIZE>(command, 0, sections.get(0))?;
        if checks.is_empty() {
            return Err(ProtocolError::count_mismatch(command, "checks", 0, 1));
        }
        let mut before: Option<[u8; 16]> = None;
        for (index, record) in checks.iter().enumerate() {
            before = Some(check_record(
                sections.offset(0),
                index,
                record,
                before.as_ref(),
            )?);
        }
        Ok(Self { checks })
    }
}

impl<'a> BodyView<'a> for CheckView<'a> {
    const COMMAND: Command = Command::Check;

    type Body = Check;

    /// Checks, in `PROTOCOL.md` §13.24's order: command header bytes 0–127 zero; the sections; section 0 a whole
    /// number of 32-byte checks ([`ProtocolError::section_length`]), at least one ([`ProtocolError::count_mismatch`]
    /// `"checks"`); each check in order: bytes 26–31 zero ([`ProtocolError::padding_not_zero`], the byte's body
    /// offset), then `session` not all zero and above the one before, compared byte by byte, `kind` 1 or 2 and
    /// `outcome` 0, 1, 12, 14 or 21 ([`ProtocolError::field_value`] `"check"`, the check's index).
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(expect_command(frame, Self::COMMAND)?, frame.body())
    }

    fn to_body(&self) -> Result<Check, ProtocolError> {
        Ok(Check::new(
            (0..self.len())
                .filter_map(|index| self.get(index))
                .collect(),
        ))
    }
}
