/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::commit::Commit;
use crate::types::commit_view::CommitView;
use crate::types::commit_view::RESULT_SIZE;
use crate::types::frame_view::FrameView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::records;

impl<'a> BodyView<'a> for CommitView<'a> {
    const COMMAND: Command = Command::Commit;

    type Body = Commit;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 24)])?;
        let sections = Sections::<1>::read(command, frame.body(), 0)?;
        let results = sections.get(0);
        for (i, record) in records(command, 0, results, RESULT_SIZE)?.enumerate() {
            let created = record.get(8).copied().unwrap_or(0);
            if created > 1 {
                return Err(ProtocolError::FieldValue(
                    command,
                    "created",
                    created.into(),
                ));
            }
            if let Some(j) = record
                .get(9..)
                .and_then(|zeros| zeros.iter().position(|b| *b != 0))
            {
                return Err(ProtocolError::PaddingNotZero(
                    command,
                    sections.offset(0) + RESULT_SIZE * i + 9 + j,
                ));
            }
        }
        Ok(Self {
            session: header.u64_at(0),
            request: header.u64_at(8),
            commit_version: header.u64_at(16),
            results,
        })
    }

    fn to_body(&self) -> Result<Commit, ProtocolError> {
        let results = (0..self.result_count())
            .filter_map(|i| self.result(i))
            .collect();
        Ok(Commit::new(
            self.session,
            self.request,
            self.commit_version,
            results,
        ))
    }
}
