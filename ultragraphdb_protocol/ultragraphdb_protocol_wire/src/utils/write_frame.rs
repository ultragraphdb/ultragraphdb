/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What DirectWrite and ConditionalWrite share (`PROTOCOL.md` §6.6): the command header.

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::command_header::{CommandHeader, command_header};
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::fields::{identity, session_request};
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
    /// none ([`ProtocolError::field_value`] `"request"`); the token as [`token::read`] reads it; `principal`, which holds
    /// any value at a role endpoint and 0 at the client endpoint (`from_client`), refused there as
    /// [`ProtocolError::field_value`] (`"principal"`).
    pub(crate) fn read(
        command: u16,
        header: CommandHeader<'_>,
        from_client: bool,
    ) -> Result<Self, ProtocolError> {
        header.unused_zero(&[(0, 64)])?;
        let session = identity(header.0, 0);
        let request = session_request(command, session, header.u64_at(16))?;
        let token = token::read(command, header, 32)?;
        let principal = header.u64_at(56);
        if from_client && principal != 0 {
            return Err(ProtocolError::field_value(command, "principal", principal));
        }
        Ok(Self {
            session,
            request,
            tenant: header.u64_at(24),
            token,
            principal,
        })
    }
}
