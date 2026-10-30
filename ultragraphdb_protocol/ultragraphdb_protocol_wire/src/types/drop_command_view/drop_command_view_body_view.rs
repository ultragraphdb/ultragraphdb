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
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;

impl<'a> BodyView<'a> for DropCommandView<'a> {
    const COMMAND: Command = Command::Drop;

    type Body = DropCommand;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 40)])?;
        let number = header.u8_at(0);
        let drop_kind = DropKind::from_number(number).ok_or(ProtocolError::FieldValue(
            command,
            "kind",
            number.into(),
        ))?;
        let namespace = header.u64_at(8);
        if drop_kind == DropKind::All && namespace != 0 {
            return Err(ProtocolError::FieldValue(command, "namespace", namespace));
        }
        let sections = Sections::<2>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (0, 1),
            sections.offset(0),
            sections.get(0),
            sections.get(1),
        )?;
        let expected = usize::from(drop_kind.names_target());
        if terms.len() != expected {
            return Err(ProtocolError::CountMismatch(
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
                    RequestTermView::BlankNode(_) => kind::BLANK_NODE,
                    RequestTermView::Literal { .. } => kind::LITERAL,
                    RequestTermView::LanguageLiteral { .. } => kind::LANGUAGE_LITERAL,
                    _ => kind::TRIPLE,
                };
                return Err(ProtocolError::FieldValue(
                    command,
                    "drop term",
                    found.into(),
                ));
            }
        };
        Ok(Self {
            kind: drop_kind,
            namespace,
            target,
            session: header.u64_at(16),
            request: header.u64_at(24),
            commit_version: header.u64_at(32),
        })
    }

    fn to_body(&self) -> Result<DropCommand, ProtocolError> {
        let drop = match (self.kind, self.target) {
            (DropKind::Predicate, Some(iri)) => DropCommand::predicate(self.namespace, iri),
            (DropKind::Graph, Some(iri)) => DropCommand::graph(self.namespace, iri),
            (DropKind::NamespaceData, _) => DropCommand::namespace_data(self.namespace),
            (DropKind::Namespace, _) => DropCommand::whole_namespace(self.namespace),
            (DropKind::All, _) => DropCommand::all(),
            // The view's check gives a predicate and a graph their target.
            (DropKind::Predicate | DropKind::Graph, None) => {
                return Err(ProtocolError::CountMismatch(
                    Self::COMMAND.number(),
                    "drop terms",
                    0,
                    1,
                ));
            }
        };
        Ok(drop
            .with_session(self.session, self.request)
            .with_commit_version(self.commit_version))
    }
}
