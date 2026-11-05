/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod direct_write_body_builder;

use ultragraphdb_core_ids::SessionToken;

use crate::types::term_table::TermTable;
use crate::types::write_change::WriteChange;
use crate::utils::write_frame::WriteHeader;

/// A changeset committed as one transaction in one frame, on a stream of its own (`PROTOCOL.md` §6.6): unconditional
/// writes, answered by a Commit or an Error, which ends the stream. Its changes take the layout of a Write's, and a
/// receiver refuses a declaration among them. Each frame has one encoding, which a receiver rebuilds byte for byte.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `session` | 16 bytes; all zero for none |
/// | 16 | `request` | u64; at least 1 with a session, 0 with none |
/// | 24 | `tenant` | u64 |
/// | 32 | `token` | session token, 24 bytes; zero for none |
/// | 56 | `principal` | u64: 0 from a client; the frontend fills it with the connection's principal when it relays |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the change records, 16 bytes each, in order, at least one |
/// | 1 | the args, u32s |
/// | 2 | the term table's records |
/// | 3 | the term table's text |
/// | 4 | the names: ends, empty |
/// | 5 | the names: text, empty |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectWrite {
    header: WriteHeader,
    terms: TermTable,
    changes: Vec<WriteChange>,
}

impl DirectWrite {
    /// A DirectWrite of `changes` naming `terms` in `tenant`, through no session, holding no token.
    pub fn new(tenant: u64, terms: TermTable, changes: Vec<WriteChange>) -> Self {
        Self::from_parts(WriteHeader::new(tenant), terms, changes)
    }

    /// The write through `session` with the next `request` number of the session's counter, from 1; a session all zero
    /// commits without exactly-once, with `request` 0.
    pub fn with_session(mut self, session: [u8; 16], request: u64) -> Self {
        self.header.session = session;
        self.header.request = request;
        self
    }

    /// The write carrying the session token the client holds, which raises the commit's epoch as a Begin's does.
    pub fn with_token(mut self, token: SessionToken) -> Self {
        self.header.token = Some(token);
        self
    }

    /// The write relayed for `principal`, the connection's the frontend fills.
    #[must_use]
    pub fn with_principal(mut self, principal: u64) -> Self {
        self.header.principal = principal;
        self
    }

    /// The principal the frontend filled; 0 from a client.
    pub fn principal(&self) -> u64 {
        self.header.principal
    }

    /// The client's session; all zero for none.
    pub fn session(&self) -> [u8; 16] {
        self.header.session
    }

    /// The request's number within its session.
    pub fn request(&self) -> u64 {
        self.header.request
    }

    /// The tenant it acts in.
    pub fn tenant(&self) -> u64 {
        self.header.tenant
    }

    /// The session token the client holds; `None` for none.
    pub fn token(&self) -> Option<&SessionToken> {
        self.header.token.as_ref()
    }

    /// The term table.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The changes, in order.
    pub fn changes(&self) -> &[WriteChange] {
        &self.changes
    }

    pub(crate) fn header(&self) -> &WriteHeader {
        &self.header
    }

    pub(crate) fn from_parts(
        header: WriteHeader,
        terms: TermTable,
        changes: Vec<WriteChange>,
    ) -> Self {
        Self {
            header,
            terms,
            changes,
        }
    }
}
