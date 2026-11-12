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
use crate::utils::body::{read_sections, u32s};
use crate::utils::command_header::expect_command;
use crate::utils::commit_operation_layout::OperationFields;
use crate::utils::fields::count;

impl<'a> BodyView<'a> for OperationView<'a> {
    const COMMAND: Command = Command::Operation;

    type Body = Operation;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 24)])?;
        let fields = OperationFields::read(header.0);
        fields.check(command)?;
        let sections = read_sections::<3>(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        let args = u32s(command, 0, sections.get(0))?;
        let expected = fields.arg_count();
        if args.len() as u64 != expected {
            return Err(ProtocolError::count_mismatch(
                command,
                "args",
                args.len() as u64,
                expected,
            ));
        }
        fields.check_args(
            command,
            (0, 0),
            &args,
            count(command, "terms", terms.len())?,
        )?;
        Ok(Self {
            fields,
            args,
            terms,
        })
    }

    fn to_body(&self) -> Result<Operation, ProtocolError> {
        Ok(Operation::new(self.terms.to_table(), self.operation()))
    }
}
