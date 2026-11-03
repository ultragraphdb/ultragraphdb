/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::report::Report;
use crate::types::report_kind::ReportKind;
use crate::types::report_view::ReportView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::fields::at_least_one;
use crate::utils::sections::{Sections, expect_command};
use crate::utils::term_kind::term_kind;

impl<'a> BodyView<'a> for ReportView<'a> {
    const COMMAND: Command = Command::Report;

    type Body = Report;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 5), (8, 24)])?;
        let partition = header.u32_at(0);
        let _partition: u64 = at_least_one(command, "partition", partition.into())?;
        let number = header.u8_at(4);
        let kind = ReportKind::from_number(number)
            .ok_or_else(|| ProtocolError::field_value(command, "kind", number.into()))?;
        let version = at_least_one(command, "version", header.u64_at(16))?;
        let sections = Sections::<2>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (0, 1),
            sections.offset(0),
            sections.get(0),
            sections.get(1),
        )?;
        let (fewest, most) = match kind {
            ReportKind::Passed => (0, 0),
            ReportKind::Refused => (1, 2),
            ReportKind::BuildFailed => (1, 1),
        };
        if !(fewest..=most).contains(&terms.len()) {
            return Err(ProtocolError::count_mismatch(
                command,
                "report terms",
                terms.len() as u64,
                most as u64,
            ));
        }
        let first = terms
            .get(0)
            .filter(|term| !matches!(term, RequestTermView::Iri(_)));
        let second = terms
            .get(1)
            .filter(|term| matches!(term, RequestTermView::Triple { .. }));
        if let Some(wrong) = first.or(second) {
            return Err(ProtocolError::field_value(
                command,
                "report term",
                term_kind(&wrong).into(),
            ));
        }
        Ok(Self {
            partition,
            kind,
            namespace: header.u64_at(8),
            version,
            terms,
        })
    }

    fn to_body(&self) -> Result<Report, ProtocolError> {
        let missing =
            || ProtocolError::count_mismatch(Self::COMMAND.number(), "report terms", 0, 1);
        match self.kind {
            ReportKind::Passed => Ok(Report::passed(self.partition, self.namespace, self.version)),
            ReportKind::BuildFailed => Ok(Report::build_failed(
                self.partition,
                self.namespace,
                self.version,
                self.predicate().ok_or_else(missing)?,
            )),
            ReportKind::Refused => Report::refused(
                self.partition,
                self.namespace,
                self.version,
                self.predicate().ok_or_else(missing)?,
                self.at_fault().ok_or_else(missing)?.to_term(),
            ),
        }
    }
}
