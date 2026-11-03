/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::scan::Scan;
use crate::types::scan_at::ScanAt;
use crate::types::scan_view::ScanView;
use crate::utils::constants::SCAN_RANGE_SIZE;
use crate::utils::fields::{epoch, identity};
use crate::utils::le::bytes_at;
use crate::utils::sections::{Sections, as_u32, expect_command, records};

/// The u32 at `at` of a range's record.
fn field(record: &[u8], at: usize) -> usize {
    bytes_at(record, at).map_or(0, u32::from_le_bytes) as usize
}

/// What the ranges checked so far leave the next: the end of the keys they hold, and the partition every range names
/// when the Scan names a transaction.
struct Cursor {
    end: usize,
    partition: Option<u32>,
}

/// Range `index`, the record `record` at body offset `at`, checked against the keys `keys` in `PROTOCOL.md` §13.13's
/// order; the cursor moves past its keys.
fn check_range(
    command: u16,
    index: usize,
    (record, at): (&[u8], usize),
    keys: &[u8],
    cursor: &mut Cursor,
) -> Result<(), ProtocolError> {
    if let Some(j) = record
        .get(4..8)
        .and_then(|zero| zero.iter().position(|b| *b != 0))
    {
        return Err(ProtocolError::padding_not_zero(command, at + 4 + j));
    }
    let partition = bytes_at(record, 0).map_or(0, u32::from_le_bytes);
    if partition == 0 || cursor.partition.is_some_and(|first| first != partition) {
        return Err(ProtocolError::field_value(
            command,
            "partition",
            partition.into(),
        ));
    }
    let (first, first_len) = (field(record, 8), field(record, 12));
    let (past, past_len) = (field(record, 16), field(record, 20));
    let range = || ProtocolError::field_value(command, "range", index as u64);
    if first != cursor.end || first_len == 0 || Some(past) != first.checked_add(first_len) {
        return Err(range());
    }
    let end = past.saturating_add(past_len);
    if end > keys.len() {
        return Err(ProtocolError::index_out_of_range(
            command,
            0,
            index,
            as_u32(end),
            as_u32(keys.len()),
        ));
    }
    let past_key = keys.get(past..end).unwrap_or_default();
    if !past_key.is_empty() && past_key <= keys.get(first..past).unwrap_or_default() {
        return Err(range());
    }
    cursor.end = end;
    Ok(())
}

impl<'a> BodyView<'a> for ScanView<'a> {
    const COMMAND: Command = Command::Scan;

    type Body = Scan;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 52)])?;
        let number = header.u8_at(0);
        let at = ScanAt::from_number(number).ok_or(ProtocolError::field_value(
            command,
            "at",
            number.into(),
        ))?;
        let read_version = header.u64_at(8);
        if !at.names_version() && read_version != 0 {
            return Err(ProtocolError::field_value(
                command,
                "read_version",
                read_version,
            ));
        }
        let wait = epoch(command, "epoch", header.u64_at(16), true)?;
        if at == ScanAt::Named && wait != 0 {
            return Err(ProtocolError::field_value(command, "epoch", wait));
        }
        let transaction = identity(header.0, 32);
        let sections = Sections::<2>::read(command, frame.body(), 0)?;
        let (ranges, keys) = (sections.get(0), sections.get(1));
        let chunks = records(command, 0, ranges, SCAN_RANGE_SIZE)?;
        if ranges.is_empty() {
            return Err(ProtocolError::count_mismatch(command, "ranges", 0, 1));
        }
        let mut cursor = Cursor {
            end: 0,
            partition: None,
        };
        for (index, record) in chunks.enumerate() {
            let at_offset = sections.offset(0) + index * SCAN_RANGE_SIZE;
            check_range(command, index, (record, at_offset), keys, &mut cursor)?;
            if transaction != [0; 16] && cursor.partition.is_none() {
                cursor.partition = Some(bytes_at(record, 0).map_or(0, u32::from_le_bytes));
            }
        }
        if cursor.end != keys.len() {
            return Err(ProtocolError::count_mismatch(
                command,
                "range keys",
                keys.len() as u64,
                cursor.end as u64,
            ));
        }
        Ok(Self {
            at,
            read_version,
            epoch: wait,
            principal: header.u64_at(24),
            transaction,
            limit: header.u32_at(48),
            ranges,
            keys,
        })
    }

    fn to_body(&self) -> Result<Scan, ProtocolError> {
        let mut scan = Scan::new(
            self.at,
            self.read_version,
            self.epoch,
            self.principal,
            self.transaction,
            self.limit,
        );
        self.ranges().for_each(|range| scan.push_range(range));
        Ok(scan)
    }
}
