/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::compute_read::ComputeRead;
use crate::types::compute_read_view::ComputeReadView;
use crate::types::frame_view::FrameView;
use crate::types::quad_pattern::QuadPattern;
use crate::types::read_level::ReadLevel;
use crate::types::term_table_view::TermTableView;
use crate::utils::command_header::CommandHeader;
use crate::utils::fields::{boolean, epoch, identity};
use crate::utils::sections::{Sections, expect_command};

/// The transaction's fields of a ComputeRead's command header, checked in `PROTOCOL.md` §13.12's order after its
/// unused bytes: `mode`, `level`, `named`, `epoch`, then `partition` and `transaction` against the mode.
struct Transaction {
    read_only: bool,
    level: ReadLevel,
    named: bool,
    epoch: u64,
    transaction: [u8; 16],
}

fn transaction(command: u16, header: CommandHeader<'_>) -> Result<Transaction, ProtocolError> {
    let read_only = match header.u8_at(4) {
        0 => false,
        1 => true,
        mode => return Err(ProtocolError::field_value(command, "mode", mode.into())),
    };
    let number = header.u8_at(5);
    let level = ReadLevel::from_number(number).ok_or(ProtocolError::field_value(
        command,
        "level",
        number.into(),
    ))?;
    let byte = header.u8_at(6);
    let named = boolean(command, "named", byte)?;
    if named && !(read_only && level == ReadLevel::Snapshot) {
        return Err(ProtocolError::field_value(command, "named", byte.into()));
    }
    let wait = epoch(command, "epoch", header.u64_at(24), true)?;
    let transaction = identity(header.0, 40);
    let bound = header.u32_at(0) != 0 && transaction != [0; 16];
    if (read_only && transaction != [0; 16]) || (!read_only && !bound) {
        return Err(ProtocolError::field_value(
            command,
            "transaction",
            u64::from(read_only),
        ));
    }
    Ok(Transaction {
        read_only,
        level,
        named,
        epoch: wait,
        transaction,
    })
}

impl<'a> BodyView<'a> for ComputeReadView<'a> {
    const COMMAND: Command = Command::ComputeRead;

    type Body = ComputeRead;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 7), (8, 64)])?;
        let fields = transaction(command, header)?;
        let sections = Sections::<4>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        let pattern = QuadPattern::read(command, 0, sections.get(0), terms.len(), false)?;
        Ok(Self {
            partition: header.u32_at(0),
            read_only: fields.read_only,
            level: fields.level,
            named: fields.named,
            tenant: header.u64_at(8),
            read_version: header.u64_at(16),
            epoch: fields.epoch,
            principal: header.u64_at(32),
            transaction: fields.transaction,
            limit: header.u32_at(56),
            deadline_ms: header.u32_at(60),
            terms,
            pattern,
            resume: sections.get(3),
        })
    }

    fn to_body(&self) -> Result<ComputeRead, ProtocolError> {
        let read = ComputeRead::new(
            self.tenant,
            self.read_only,
            self.level,
            self.terms.to_table(),
            self.pattern,
            self.resume.to_vec(),
        )
        .with_partition(self.partition)
        .with_transaction(self.transaction)
        .with_epoch(self.epoch)
        .with_principal(self.principal)
        .with_limit(self.limit)
        .with_deadline_ms(self.deadline_ms);
        Ok(if self.named {
            read.with_named_version(self.read_version)
        } else {
            read.with_read_version(self.read_version)
        })
    }
}
