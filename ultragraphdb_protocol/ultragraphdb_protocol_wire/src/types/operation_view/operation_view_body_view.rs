/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::operation::Operation;
use crate::types::operation_view::OperationView;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::ABSENT;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::{as_u32, u32s};

impl<'a> BodyView<'a> for OperationView<'a> {
    const COMMAND: Command = Command::Operation;

    type Body = Operation;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 24)])?;
        let kind = header.u8_at(0);
        let delta = header.u64_at(8) as i64;
        let properties = header.u32_at(16);
        let links = header.u32_at(20);
        if !(1..=4).contains(&kind) {
            return Err(ProtocolError::FieldValue(command, "kind", kind.into()));
        }
        if delta != 0 && kind != 4 {
            return Err(ProtocolError::FieldValue(command, "delta", delta as u64));
        }
        if properties != 0 && kind > 2 {
            return Err(ProtocolError::FieldValue(
                command,
                "properties",
                properties.into(),
            ));
        }
        if links != 0 && kind != 2 {
            return Err(ProtocolError::FieldValue(command, "links", links.into()));
        }
        let sections = Sections::<3>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        let args = u32s(command, 0, sections.get(0))?;
        let expected = match kind {
            1 | 2 => 3 + 2 * u64::from(properties) + 3 * u64::from(links),
            3 => 5,
            _ => 3,
        };
        if args.len() as u64 != expected {
            return Err(ProtocolError::CountMismatch(
                command,
                "args",
                args.len() as u64,
                expected,
            ));
        }
        let limit = as_u32(terms.len());
        let link_start = 3 + 2 * properties as usize;
        for (at, arg) in args.iter().enumerate() {
            let direction = kind == 2 && at >= link_start && (at - link_start) % 3 == 2;
            if direction {
                if *arg > 1 {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "link direction",
                        (*arg).into(),
                    ));
                }
            } else if !(at == 0 && *arg == ABSENT) && *arg >= limit {
                return Err(ProtocolError::IndexOutOfRange(command, 0, at, *arg, limit));
            }
        }
        Ok(Self {
            kind,
            delta,
            properties,
            links,
            args,
            terms,
        })
    }

    fn to_body(&self) -> Result<Operation, ProtocolError> {
        Ok(Operation::new(self.terms.to_table(), self.operation()))
    }
}
