/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::column_type::ColumnType;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::pattern_slot::PatternSlot;
use crate::types::prepare::Prepare;
use crate::types::prepare_view::PrepareView;
use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table_view::TermTableView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;

impl<'a> BodyView<'a> for PrepareView<'a> {
    const COMMAND: Command = Command::Prepare;

    type Body = Prepare;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 24)])?;
        let sections = Sections::<5>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        let pattern = QuadPattern::read(command, 0, sections.get(0), terms.len(), true)?;
        let types = |bytes: &[u8]| -> Result<Vec<ColumnType>, ProtocolError> {
            bytes
                .iter()
                .map(|b| {
                    ColumnType::from_number(*b).ok_or(ProtocolError::FieldValue(
                        command,
                        "column type",
                        (*b).into(),
                    ))
                })
                .collect()
        };
        let parameter_types = types(sections.get(3))?;
        let result_types = types(sections.get(4))?;
        if parameter_types.len() != pattern.parameters() {
            return Err(ProtocolError::CountMismatch(
                command,
                "parameter types",
                parameter_types.len() as u64,
                pattern.parameters() as u64,
            ));
        }
        let unbound = pattern
            .slots()
            .iter()
            .filter(|slot| matches!(slot, PatternSlot::Unbound))
            .count();
        if !result_types.is_empty() && result_types.len() != unbound {
            return Err(ProtocolError::CountMismatch(
                command,
                "result types",
                result_types.len() as u64,
                unbound as u64,
            ));
        }
        Ok(Self {
            namespace: header.u64_at(0),
            statement: header.u64_at(8),
            schema: header.u64_at(16),
            terms,
            pattern,
            parameter_types,
            result_types,
        })
    }

    fn to_body(&self) -> Result<Prepare, ProtocolError> {
        Ok(Prepare::new(
            self.namespace,
            self.statement,
            self.schema,
            self.terms.to_table(),
            self.pattern,
            self.parameter_types.clone(),
            self.result_types.clone(),
        ))
    }
}
