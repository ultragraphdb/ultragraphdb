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
use crate::types::key_values::KeyValues;
use crate::types::record_kind::RecordKind;
use crate::types::round_body::{DECISION_SIZE, RECORD_ENTRY_SIZE, RoundBody};
use crate::types::round_header::RoundHeader;
use crate::types::round_view::RoundView;
use crate::types::write_set_record_view::WriteSetRecordView;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;
use crate::utils::sections::{Sections, align8, as_u32, expect_command, records};

/// The sections of a Round body: decisions, the record table, the records, the dictionary entries.
const ROUND_SECTIONS: usize = 4;

/// The largest sequence a version takes within its epoch, 2²² − 2 (`PROTOCOL.md` §1).
const SEQUENCE_MAX: u64 = (1 << 22) - 2;

impl<'a> BodyView<'a> for RoundView<'a> {
    const COMMAND: Command = Command::Round;

    type Body = RoundBody;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let header = expect_command(frame, Self::COMMAND)?;
        Self::from_body(RoundHeader::decode(header.0)?, frame.body())
    }

    fn to_body(&self) -> Result<RoundBody, ProtocolError> {
        let mut round = RoundBody::new(
            self.header.position(),
            self.header.parent(),
            self.header.incarnation(),
            self.header.first_version(),
            self.header.derivation(),
        );
        let mut records = self.records.iter();
        for decision in &self.decisions {
            match decision {
                Decision::Committed => {
                    let record = records.next().ok_or(ProtocolError::count_mismatch(
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
        for entry in self.dictionary.iter() {
            round.insert_dictionary(
                entry.key().to_vec(),
                entry.value().unwrap_or_default().to_vec(),
            );
        }
        Ok(round)
    }
}

impl<'a> RoundView<'a> {
    /// The round `body` holds under `round`, its command header: the body's bytes before its chunk table, checked as
    /// [`BodyView::from_frame`] checks a frame's, as the log hands a round back without its frame.
    ///
    /// # Errors
    ///
    /// In order (`PROTOCOL.md` §12.1): the section table (§4.1's refusals); [`ProtocolError::section_length`] for
    /// decisions not a whole number of 16 bytes; [`ProtocolError::count_mismatch`] `"decisions"` for a count other than
    /// `transactions`; per decision, [`ProtocolError::padding_not_zero`] for bytes 2 to 3, a commit's `record` not the
    /// commits before it ([`ProtocolError::field_value`] `"record"`) or its `detail` not 0 (`"detail"`), an abort's
    /// `outcome` no error code (`"outcome"`) or its `record` not [`crate::ABSENT`] (`"record"`); the last committed
    /// entry's version outside `first_version`'s epoch or past its last sequence (`"first_version"`); the record table
    /// not a whole number of 8 bytes ([`ProtocolError::section_length`]) or not one entry per commit
    /// ([`ProtocolError::count_mismatch`] `"records"`); per entry, outside section 2
    /// ([`ProtocolError::index_out_of_range`]), not at the expected offset (`"record offset"`), padding not zero, the
    /// record refused ([`ProtocolError::record`], wrapping its refusal), a tenant's records, registrations aside,
    /// naming more than one catalog (`"catalog"`); bytes after the last record ([`ProtocolError::trailing_bytes`]); the
    /// dictionary's entries each inside the section ([`ProtocolError::entry_out_of_bounds`]) with zero padding, every
    /// value present (`"dictionary entry"`), each key above the one before (`"dictionary order"`, naming the entry).
    pub fn from_body(round: RoundHeader, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Round.number();
        let sections = Sections::<ROUND_SECTIONS>::read(command, body, 0)?;
        let (decisions, committed) = read_decisions(command, &sections, round.transactions())?;
        check_first_version(command, round.first_version(), committed)?;
        let records = read_records(command, &sections, committed)?;
        let dictionary = read_dictionary(command, &sections)?;
        Ok(Self {
            header: round,
            decisions,
            records,
            dictionary,
        })
    }
}

/// The decisions of section 0, `transactions` of them, and how many committed: each committed one naming the next
/// record and no detail, each aborted one a known code and no record.
fn read_decisions(
    command: u16,
    sections: &Sections<'_, ROUND_SECTIONS>,
    transactions: u32,
) -> Result<(Vec<Decision>, u32), ProtocolError> {
    let decision_bytes = records(command, 0, sections.get(0), DECISION_SIZE)?;
    if decision_bytes.len() as u64 != u64::from(transactions) {
        return Err(ProtocolError::count_mismatch(
            command,
            "decisions",
            decision_bytes.len() as u64,
            transactions.into(),
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
            return Err(ProtocolError::padding_not_zero(
                command,
                sections.offset(0) + DECISION_SIZE * i + 2 + j,
            ));
        }
        if outcome == 0 {
            if record != committed {
                return Err(ProtocolError::field_value(command, "record", record.into()));
            }
            if detail != 0 {
                return Err(ProtocolError::field_value(command, "detail", detail));
            }
            committed = committed.saturating_add(1);
            decisions.push(Decision::Committed);
        } else {
            let code = ErrorCode::from_number(outcome).ok_or(ProtocolError::field_value(
                command,
                "outcome",
                outcome.into(),
            ))?;
            if record != ABSENT {
                return Err(ProtocolError::field_value(command, "record", record.into()));
            }
            decisions.push(Decision::Aborted { code, detail });
        }
    }
    Ok((decisions, committed))
}

/// The round's `committed` entries take versions `first_version + 2k`: the last within `first_version`'s epoch and
/// at most its last sequence, so the round carries one epoch.
fn check_first_version(
    command: u16,
    first_version: u64,
    committed: u32,
) -> Result<(), ProtocolError> {
    let Some(steps) = committed.checked_sub(1) else {
        return Ok(());
    };
    let last = u64::from(steps)
        .checked_mul(2)
        .and_then(|span| first_version.checked_add(span));
    match last {
        Some(last) if last >> 22 == first_version >> 22 && last & 0x3F_FFFF <= SEQUENCE_MAX => {
            Ok(())
        }
        Some(_) | None => Err(ProtocolError::field_value(
            command,
            "first_version",
            first_version,
        )),
    }
}

/// The `committed` records section 1's table places in section 2's data, each 8-aligned after the last with zero
/// padding between, each valid, the records of a tenant naming one catalog, and nothing after the last.
fn read_records<'a>(
    command: u16,
    sections: &Sections<'a, ROUND_SECTIONS>,
    committed: u32,
) -> Result<Vec<WriteSetRecordView<'a>>, ProtocolError> {
    let table = records(command, 1, sections.get(1), RECORD_ENTRY_SIZE)?;
    if table.len() as u64 != u64::from(committed) {
        return Err(ProtocolError::count_mismatch(
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
    let mut catalogs: Vec<(u64, u64)> = Vec::new();
    for (index, entry) in table.enumerate() {
        let offset = bytes_at(entry, 0).map_or(0, u32::from_le_bytes);
        let len = bytes_at(entry, 4).map_or(0, u32::from_le_bytes);
        let start = offset as usize;
        let stop = start.saturating_add(len as usize);
        if stop > data.len() {
            return Err(ProtocolError::index_out_of_range(
                command,
                1,
                index,
                as_u32(stop),
                as_u32(data.len()),
            ));
        }
        if start != align8(end) {
            return Err(ProtocolError::field_value(
                command,
                "record offset",
                offset.into(),
            ));
        }
        if let Some(j) = data
            .get(end..start)
            .and_then(|p| p.iter().position(|b| *b != 0))
        {
            return Err(ProtocolError::padding_not_zero(command, base + end + j));
        }
        let view = WriteSetRecordView::new(data.get(start..stop).unwrap_or_default())
            .map_err(|cause| ProtocolError::record(index, cause))?;
        check_catalog(command, &mut catalogs, &view)?;
        views.push(view);
        end = stop;
    }
    if end != data.len() {
        return Err(ProtocolError::trailing_bytes(
            command,
            base + end,
            base + data.len(),
        ));
    }
    Ok(views)
}

/// `record`'s catalog against the one its tenant's records named before it, in `catalogs`, which it joins; a
/// registration names none.
fn check_catalog(
    command: u16,
    catalogs: &mut Vec<(u64, u64)>,
    record: &WriteSetRecordView<'_>,
) -> Result<(), ProtocolError> {
    if record.kind() == RecordKind::Registration {
        return Ok(());
    }
    match catalogs
        .iter()
        .find(|(tenant, _)| *tenant == record.tenant())
    {
        Some((_, catalog)) if *catalog != record.catalog() => Err(ProtocolError::field_value(
            command,
            "catalog",
            record.catalog(),
        )),
        Some(_) => Ok(()),
        None => {
            catalogs.push((record.tenant(), record.catalog()));
            Ok(())
        }
    }
}

/// Section 3's dictionary entries: each framed, its value present, its key above the one before.
fn read_dictionary<'a>(
    command: u16,
    sections: &Sections<'a, ROUND_SECTIONS>,
) -> Result<KeyValues<'a>, ProtocolError> {
    let dictionary = KeyValues::read(command, 3, sections.offset(3), sections.get(3))?;
    let mut previous: Option<&[u8]> = None;
    for (index, entry) in dictionary.iter().enumerate() {
        if entry.value().is_none() {
            return Err(ProtocolError::field_value(
                command,
                "dictionary entry",
                ABSENT.into(),
            ));
        }
        if previous.is_some_and(|key| entry.key() <= key) {
            return Err(ProtocolError::field_value(
                command,
                "dictionary order",
                index as u64,
            ));
        }
        previous = Some(entry.key());
    }
    Ok(dictionary)
}
