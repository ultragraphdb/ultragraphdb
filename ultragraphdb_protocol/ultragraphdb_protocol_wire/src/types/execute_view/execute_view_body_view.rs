/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::execute::Execute;
use crate::types::execute_view::ExecuteView;
use crate::types::frame_view::FrameView;
use crate::types::term_table_view::TermTableView;
use crate::utils::sections::Sections;
use crate::utils::sections::{as_u32, u32s};
use crate::utils::sections::{expect_command, statement_ids_flag};

impl<'a> BodyView<'a> for ExecuteView<'a> {
    const COMMAND: Command = Command::Execute;

    type Body = Execute;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 24)])?;
        let statement_ids = statement_ids_flag(command, header.u32_at(20))?;
        let sections = Sections::<4>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (0, 1),
            sections.offset(0),
            sections.get(0),
            sections.get(1),
        )?;
        let parameters = u32s(command, 2, sections.get(2))?;
        let limit = as_u32(terms.len());
        if let Some((at, index)) = parameters.iter().enumerate().find(|(_, i)| **i >= limit) {
            return Err(ProtocolError::IndexOutOfRange(
                command, 2, at, *index, limit,
            ));
        }
        Ok(Self {
            statement: header.u64_at(0),
            transaction: header.u64_at(8),
            limit: header.u32_at(16),
            statement_ids,
            terms,
            parameters,
            resume: sections.get(3),
        })
    }

    fn to_body(&self) -> Result<Execute, ProtocolError> {
        let execute = Execute::new(
            self.statement,
            self.transaction,
            self.limit,
            self.terms.to_table(),
            self.parameters.clone(),
            self.resume.to_vec(),
        );
        Ok(if self.statement_ids {
            execute.with_statement_ids()
        } else {
            execute
        })
    }
}
