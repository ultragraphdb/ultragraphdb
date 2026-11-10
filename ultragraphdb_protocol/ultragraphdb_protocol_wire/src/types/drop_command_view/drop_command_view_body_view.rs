/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::drop_command::DropCommand;
use crate::types::drop_command_view::DropCommandView;
use crate::types::drop_kind::DropKind;
use crate::types::frame_view::FrameView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table::kind;
use crate::types::term_table_view::TermTableView;
use crate::utils::body::read_sections;
use crate::utils::command_header::expect_command;
use crate::utils::fields::{identity, session_request};
use crate::utils::token;

impl<'a> BodyView<'a> for DropCommandView<'a> {
    const COMMAND: Command = Command::Drop;

    type Body = DropCommand;

    /// Checks, in `PROTOCOL.md` §6.5's order: unused bytes 1–7 and 80–127 zero; `request` at least 1 with a session, 0
    /// with none; `kind`; `catalog`, 0 for kinds 1 and 2; the token, of the reply's epoch through a session and none
    /// through no session; the sections; the term table and its entry.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 80)])?;
        let session = identity(header.0, 16);
        let request = session_request(command, session, header.u64_at(32))?;
        let number = header.u8_at(0);
        let drop_kind = DropKind::from_number(number).ok_or(ProtocolError::field_value(
            command,
            "kind",
            number.into(),
        ))?;
        let catalog = header.u64_at(72);
        if catalog != 0 && drop_kind.names_target() {
            return Err(ProtocolError::field_value(command, "catalog", catalog));
        }
        let tenant = header.u64_at(8);
        let commit_version = header.u64_at(40);
        let token = token::commit_token(command, header, 48, commit_version, session)?;
        let sections = read_sections::<2>(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (0, 1),
            sections.offset(0),
            sections.get(0),
            sections.get(1),
        )?;
        let expected = usize::from(drop_kind.names_target());
        if terms.len() != expected {
            return Err(ProtocolError::count_mismatch(
                command,
                "drop terms",
                terms.len() as u64,
                expected as u64,
            ));
        }
        let target = match terms.get(0) {
            None => None,
            Some(RequestTermView::Iri(iri)) => Some(iri),
            Some(other) => {
                let found = match other {
                    RequestTermView::Iri(_) => kind::IRI,
                    RequestTermView::BlankNode(_) => kind::BLANK_NODE,
                    RequestTermView::Literal { .. } => kind::LITERAL,
                    RequestTermView::LanguageLiteral { .. } => kind::LANGUAGE_LITERAL,
                    RequestTermView::Triple { .. } => kind::TRIPLE,
                };
                return Err(ProtocolError::field_value(
                    command,
                    "drop term",
                    found.into(),
                ));
            }
        };
        Ok(Self {
            kind: drop_kind,
            tenant,
            target,
            session,
            request,
            commit_version,
            token,
            catalog,
        })
    }

    fn to_body(&self) -> Result<DropCommand, ProtocolError> {
        let drop = match (self.kind, self.target) {
            (DropKind::Predicate, Some(iri)) => DropCommand::predicate(self.tenant, iri),
            (DropKind::Graph, Some(iri)) => DropCommand::graph(self.tenant, iri),
            (DropKind::TenantData, _) => DropCommand::tenant_data(self.tenant),
            (DropKind::Tenant, _) => DropCommand::whole_tenant(self.tenant),
            // The view's check gives a predicate and a graph their target.
            (DropKind::Predicate | DropKind::Graph, None) => {
                return Err(ProtocolError::count_mismatch(
                    Self::COMMAND.number(),
                    "drop terms",
                    0,
                    1,
                ));
            }
        };
        let drop = drop
            .with_session(self.session, self.request)
            .with_commit_version(self.commit_version)
            .with_catalog(self.catalog);
        Ok(match &self.token {
            Some(token) => drop.with_token(token.clone()),
            None => drop,
        })
    }
}
