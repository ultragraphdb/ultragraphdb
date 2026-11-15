/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{u32_bytes, write_sections};
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::declare::Declare;
use crate::types::declare_view::DeclareView;
use crate::types::drop_kind::DropKind;
use crate::types::header::Header;
use crate::types::term_table::TermTable;
use crate::utils::changes::ChangeWriter;
use crate::utils::checked_frame::checked_frame;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

/// `kind` of a Declare of a transaction's declarations.
const DECLARATIONS: u8 = 1;

/// `kind` of a Declare of a Drop's removal.
const DROP: u8 = 2;

impl BodyBuilder for Declare {
    const COMMAND: Command = Command::Declare;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        let (kind, drop) = self
            .drop_kind()
            .map_or((DECLARATIONS, 0), |drop| (DROP, DropKind::number(drop)));
        command_header(&[
            (0, &[kind, drop]),
            (8, &self.tenant().to_le_bytes()),
            (16, &self.built_on().to_le_bytes()),
            (24, &self.principal().to_le_bytes()),
            (32, &self.version().to_le_bytes()),
            (40, &self.commit_version().to_le_bytes()),
        ])
    }

    /// The six sections of a Write (`PROTOCOL.md` §7.2): the declarations' records, their args and their names, and
    /// the term table of a Drop's target.
    fn body(&self) -> Vec<u8> {
        let mut writer = ChangeWriter::default();
        for declaration in self.declared() {
            writer.declaration(declaration, 0);
        }
        let mut terms = TermTable::new();
        if let Some(target) = self.target() {
            let _index: u32 = terms.iri(target);
        }
        let (records, text) = terms.sections();
        let (ends, names) = writer.names.sections();
        write_sections(
            &[],
            &[
                &writer.records,
                &u32_bytes(&writer.args),
                &records,
                &text,
                &ends,
                &names,
            ],
        )
    }

    /// The frame of the Declare under `header`; a body [`crate::DeclareView`] refuses is refused the same way, the
    /// refusals [`Declare`] lists.
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        checked_frame(
            Self::COMMAND,
            &header,
            &self.command_header(),
            &self.body(),
            DeclareView::read,
        )
    }
}
