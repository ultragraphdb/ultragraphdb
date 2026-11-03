/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::commit::Commit;
use crate::types::commit_view::CommitView;
use crate::types::frame_view::FrameView;
use crate::types::term_table::kind;
use crate::types::term_table_view::TermTableView;
use crate::utils::fields::identity;
use crate::utils::results::check_results;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::term_kind::term_kind;
use crate::utils::token;

impl<'a> BodyView<'a> for CommitView<'a> {
    const COMMAND: Command = Command::Commit;

    type Body = Commit;

    /// Checks, in `PROTOCOL.md` §6.3's order: unused bytes 56–127 zero; the token, zero in a request (`commit_version`
    /// 0) and in a reply a token of epoch `commit_version >> 22`; the sections; the term table, every entry an IRI; in
    /// a request, every section empty; the results.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 56)])?;
        let commit_version = header.u64_at(24);
        let token = token::commit_token(command, header, 32, commit_version)?;
        let reply = commit_version != 0;
        let sections = Sections::<3>::read(command, frame.body(), 0)?;
        let nodes = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        if let Some(found) = (0..nodes.len())
            .filter_map(|i| nodes.get(u32::try_from(i).ok()?))
            .map(|term| term_kind(&term))
            .find(|k| *k != kind::IRI)
        {
            return Err(ProtocolError::field_value(
                command,
                "node term",
                found.into(),
            ));
        }
        let results = sections.get(0);
        if !reply && !(results.is_empty() && nodes.is_empty()) {
            return Err(ProtocolError::count_mismatch(
                command,
                "results",
                (results.len() / 16 + nodes.len()) as u64,
                0,
            ));
        }
        check_results(command, 0, sections.offset(0), results, Some(nodes.len()))?;
        Ok(Self {
            session: identity(header.0, 0),
            request: header.u64_at(16),
            commit_version,
            token,
            results,
            nodes,
        })
    }

    fn to_body(&self) -> Result<Commit, ProtocolError> {
        let mut commit = Commit::new(self.session, self.request, self.commit_version);
        if let Some(token) = &self.token {
            commit = commit.with_token(token.clone());
        }
        let results = (0..self.result_count())
            .filter_map(|i| self.result(i))
            .collect();
        commit.set_nodes(self.nodes.to_table(), results);
        Ok(commit)
    }
}
