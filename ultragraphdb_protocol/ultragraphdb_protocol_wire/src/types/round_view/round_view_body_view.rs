/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::decision::Decision;
use crate::types::error_code::ErrorCode;
use crate::types::frame_view::FrameView;
use crate::types::round_body::RoundBody;
use crate::types::round_body::{DECISION_SIZE, RECORD_ENTRY_SIZE};
use crate::types::round_header::RoundHeader;
use crate::types::round_view::RoundView;
use crate::types::write_set_record_view::WriteSetRecordView;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::{align8, as_u32, records};

impl<'a> BodyView<'a> for RoundView<'a> {
    const COMMAND: Command = Command::Round;

    type Body = RoundBody;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 36)])?;
        Self::from_body(RoundHeader::decode(header.0), frame.body())
    }

    fn to_body(&self) -> Result<RoundBody, ProtocolError> {
        let mut round = RoundBody::new(
            self.header.position(),
            self.header.parent(),
            self.header.incarnation(),
            self.header.first_version(),
        );
        let mut records = self.records.iter();
        for decision in &self.decisions {
            match decision {
                Decision::Committed => {
                    let record = records.next().ok_or(ProtocolError::CountMismatch(
                        Self::COMMAND.number(),
                        "records",
                        self.records.len() as u64,
                        self.decisions.len() as u64,
                    ))?;
                    round.commit(record.bytes().to_vec());
                }
                Decision::Aborted { code, detail } => round.abort(*code, *detail),
            }
        }
        Ok(round)
    }
}

impl<'a> RoundView<'a> {
    /// The round `body` holds under `round`, its command header: the body's bytes before its chunk table, checked as
    /// [`BodyView::from_frame`] checks a frame's, as the log hands a round back without its frame (7.9).
    ///
    /// # Errors
    ///
    /// As [`BodyView::from_frame`]'s, after its header's.
    pub fn from_body(round: RoundHeader, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Round.number();
        let sections = Sections::<3>::read(command, body, 0)?;
        let decision_bytes = records(command, 0, sections.get(0), DECISION_SIZE)?;
        if decision_bytes.len() as u64 != u64::from(round.transactions()) {
            return Err(ProtocolError::CountMismatch(
                command,
                "decisions",
                decision_bytes.len() as u64,
                round.transactions().into(),
            ));
        }
        let mut decisions = Vec::with_capacity(decision_bytes.len());
        let mut committed = 0u32;
        for (i, d) in decision_bytes.enumerate() {
            let outcome = bytes_at(d, 0).map_or(0, u16::from_le_bytes);
            let record = bytes_at(d, 4).map_or(0, u32::from_le_bytes);
            let detail = bytes_at(d, 8).map_or(0, u64::from_le_bytes);
            if let Some(j) = d
                .get(2..4)
                .and_then(|zeros| zeros.iter().position(|b| *b != 0))
            {
                return Err(ProtocolError::PaddingNotZero(
                    command,
                    sections.offset(0) + DECISION_SIZE * i + 2 + j,
                ));
            }
            if outcome == 0 {
                if record != committed {
                    return Err(ProtocolError::FieldValue(command, "record", record.into()));
                }
                if detail != 0 {
                    return Err(ProtocolError::FieldValue(command, "detail", detail));
                }
                committed += 1;
                decisions.push(Decision::Committed);
            } else {
                let code = ErrorCode::from_number(outcome).ok_or(ProtocolError::FieldValue(
                    command,
                    "outcome",
                    outcome.into(),
                ))?;
                if record != ABSENT {
                    return Err(ProtocolError::FieldValue(command, "record", record.into()));
                }
                decisions.push(Decision::Aborted { code, detail });
            }
        }
        let table = records(command, 1, sections.get(1), RECORD_ENTRY_SIZE)?;
        if table.len() as u64 != u64::from(committed) {
            return Err(ProtocolError::CountMismatch(
                command,
                "records",
                table.len() as u64,
                committed.into(),
            ));
        }
        let data = sections.get(2);
        let base = sections.offset(2);
        let mut end = 0usize;
        let mut views = Vec::with_capacity(table.len());
        for (index, entry) in table.enumerate() {
            let offset = bytes_at(entry, 0).map_or(0, u32::from_le_bytes);
            let len = bytes_at(entry, 4).map_or(0, u32::from_le_bytes);
            let start = offset as usize;
            let stop = start.saturating_add(len as usize);
            if stop > data.len() {
                return Err(ProtocolError::IndexOutOfRange(
                    command,
                    1,
                    index,
                    as_u32(stop),
                    as_u32(data.len()),
                ));
            }
            if start != align8(end) {
                return Err(ProtocolError::FieldValue(
                    command,
                    "record offset",
                    offset.into(),
                ));
            }
            if let Some(j) = data
                .get(end..start)
                .and_then(|p| p.iter().position(|b| *b != 0))
            {
                return Err(ProtocolError::PaddingNotZero(command, base + end + j));
            }
            let view = WriteSetRecordView::new(data.get(start..stop).unwrap_or_default())
                .map_err(|cause| ProtocolError::Record(index, cause))?;
            views.push(view);
            end = stop;
        }
        if end != data.len() {
            return Err(ProtocolError::TrailingBytes(
                command,
                base + end,
                base + data.len(),
            ));
        }
        Ok(Self {
            header: round,
            decisions,
            records: views,
        })
    }
}
