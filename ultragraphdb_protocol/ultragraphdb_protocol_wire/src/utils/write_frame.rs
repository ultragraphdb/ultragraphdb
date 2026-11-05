/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What DirectWrite and ConditionalWrite share (`PROTOCOL.md` §6.6): the command header and sections 0 to 5.

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::string_list::StringList;
use crate::types::term_table_view::TermTableView;
use crate::types::write_change::WriteChange;
use crate::utils::changes::{CHANGE_RECORD_SIZE, decode_change};
use crate::utils::command_header::{CommandHeader, command_header};
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::fields::{identity, session_request};
use crate::utils::sections::{Sections, u32s};
use crate::utils::token;

/// The command header: `session` 16 bytes, `request` u64, `tenant` u64, `token` 24 bytes, `principal` u64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WriteHeader {
    pub(crate) session: [u8; 16],
    pub(crate) request: u64,
    pub(crate) tenant: u64,
    pub(crate) token: Option<SessionToken>,
    pub(crate) principal: u64,
}

impl WriteHeader {
    /// The header of a write in `tenant`, through no session, holding no token.
    pub(crate) fn new(tenant: u64) -> Self {
        Self {
            session: [0; 16],
            request: 0,
            tenant,
            token: None,
            principal: 0,
        }
    }

    /// The header's 128 bytes.
    pub(crate) fn encode(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.session),
            (16, &self.request.to_le_bytes()),
            (24, &self.tenant.to_le_bytes()),
            (32, &token::encode(self.token.as_ref())),
            (56, &self.principal.to_le_bytes()),
        ])
    }

    /// `header` checked in the protocol's order: unused bytes 64 to 127 zero; `request` at least 1 with a session, 0 with
    /// none ([`ProtocolError::field_value`] `"request"`); the token as [`token::read`] reads it. `principal` holds any value
    /// until the client endpoint's [`WriteHeader::check_from_client`] holds it to 0.
    pub(crate) fn read(command: u16, header: CommandHeader<'_>) -> Result<Self, ProtocolError> {
        header.unused_zero(&[(0, 64)])?;
        let session = identity(header.0, 0);
        Ok(Self {
            session,
            request: session_request(command, session, header.u64_at(16))?,
            tenant: header.u64_at(24),
            token: token::read(command, header, 32)?,
            principal: header.u64_at(56),
        })
    }

    /// The rule the client endpoint holds a write to: `principal` 0, refused as [`ProtocolError::field_value`]
    /// (`"principal"`).
    pub(crate) fn check_from_client(&self, command: u16) -> Result<(), ProtocolError> {
        if self.principal != 0 {
            return Err(ProtocolError::field_value(
                command,
                "principal",
                self.principal,
            ));
        }
        Ok(())
    }
}

/// Sections 0 to 5 of a received body: the term table and the args checked, the names found empty; the records the
/// caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WriteSections<'a> {
    pub(crate) records: &'a [u8],
    pub(crate) args: Vec<u32>,
    pub(crate) terms: TermTableView<'a>,
}

impl<'a> WriteSections<'a> {
    /// `sections` checked in the protocol's order: the term table (2, 3); the names (4, 5), a string list holding no name
    /// ([`ProtocolError::count_mismatch`] `"names"`, the names' count against 0); the args a whole number of u32s
    /// ([`ProtocolError::section_length`], section 1).
    pub(crate) fn read<const N: usize>(
        command: u16,
        sections: &Sections<'a, N>,
    ) -> Result<Self, ProtocolError> {
        let terms = TermTableView::read(
            command,
            (2, 3),
            sections.offset(2),
            sections.get(2),
            sections.get(3),
        )?;
        let names = StringList::read(command, (4, 5), sections.get(4), sections.get(5))?;
        if !names.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "names",
                names.len() as u64,
                0,
            ));
        }
        let args = u32s(command, 1, sections.get(1))?;
        Ok(Self {
            records: sections.get(0),
            args,
            terms,
        })
    }

    /// The number of whole change records.
    pub(crate) fn len(&self) -> usize {
        self.records.len() / CHANGE_RECORD_SIZE
    }

    /// Change `index` of the checked records, `None` past the last.
    pub(crate) fn change(&self, index: usize) -> Option<WriteChange> {
        decode_change(self.records, index, &self.args, &StringList::EMPTY)
    }

    /// The changes of the checked records, in order.
    pub(crate) fn changes(&self) -> Vec<WriteChange> {
        (0..self.len())
            .filter_map(|index| self.change(index))
            .collect()
    }
}
