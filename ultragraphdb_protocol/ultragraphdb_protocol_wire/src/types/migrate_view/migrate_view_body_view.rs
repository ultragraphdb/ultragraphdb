/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::migrate::Migrate;
use crate::types::migrate_action::MigrateAction;
use crate::types::migrate_state::MigrateState;
use crate::types::migrate_view::MigrateView;
use crate::utils::body::{read_sections, text};
use crate::utils::command_header::expect_command;
use crate::utils::fields::{at_least_one, epoch};

/// The UGS refusal numbers a failed run's check names: `UGS2201` to `UGS2299` (`PROTOCOL.md` §8.6).
const REFUSALS: std::ops::RangeInclusive<u32> = 2_201..=2_299;

impl<'a> BodyView<'a> for MigrateView<'a> {
    const COMMAND: Command = Command::Migrate;

    type Body = Migrate;

    /// Checks, in `PROTOCOL.md` §8.6's order: unused bytes 18–19 and 32–127 zero; `staged` at least 1; `action` 1 to 5;
    /// `state`, 0 in a request and 1 to 9 in a reply; `refusal`, 0 unless `state` is 9 and then 0 or 2201 to 2299;
    /// `epoch`, 0 in a request and at most 2⁴² − 1 in a reply; the sections; in a request, both sections empty
    /// ([`ProtocolError::count_mismatch`] `"migrate"`); sections 0 and 1 UTF-8; section 0 empty when `refusal` is 0
    /// ([`ProtocolError::count_mismatch`] `"message"`).
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 18), (20, 32)])?;
        let staged = at_least_one(command, "staged", header.u64_at(8))?;
        let number = header.u8_at(16);
        let action = MigrateAction::from_number(number)
            .ok_or_else(|| ProtocolError::field_value(command, "action", number.into()))?;
        let number = header.u8_at(17);
        let state = match number {
            0 => None,
            _ => Some(
                MigrateState::from_number(number)
                    .ok_or_else(|| ProtocolError::field_value(command, "state", number.into()))?,
            ),
        };
        let refusal = header.u32_at(20);
        let failed = state == Some(MigrateState::Failed);
        if refusal != 0 && !(failed && REFUSALS.contains(&refusal)) {
            return Err(ProtocolError::field_value(
                command,
                "refusal",
                refusal.into(),
            ));
        }
        let entered = epoch(command, "epoch", header.u64_at(24), true)?;
        if state.is_none() && entered != 0 {
            return Err(ProtocolError::field_value(command, "epoch", entered));
        }
        let sections = read_sections::<2>(command, frame.body(), 0)?;
        let (message, report) = (sections.get(0), sections.get(1));
        if state.is_none()
            && let Some(held) = [message, report].iter().find(|s| !s.is_empty())
        {
            return Err(ProtocolError::count_mismatch(
                command,
                "migrate",
                held.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        }
        let message = text(command, 0, message)?;
        let report = text(command, 1, report)?;
        if refusal == 0 && !message.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "message",
                message.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        }
        Ok(Self {
            tenant: header.u64_at(0),
            staged,
            action,
            state,
            refusal,
            epoch: entered,
            message,
            report,
        })
    }

    fn to_body(&self) -> Result<Migrate, ProtocolError> {
        let request = Migrate::new(self.tenant, self.staged, self.action);
        let answered = match self.state {
            Some(state) => request.answered(state, self.epoch, self.report),
            None => request,
        };
        Ok(match self.refusal {
            0 => answered,
            refusal => answered.refused(refusal, self.message),
        })
    }
}
