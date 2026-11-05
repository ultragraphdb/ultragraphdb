/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod conditional_write_body_builder;

use ultragraphdb_core_ids::SessionToken;

use crate::types::commit_operation::CommitOperation;
use crate::types::term_table::TermTable;
use crate::types::write_change::WriteChange;
use crate::utils::write_frame::WriteHeader;

/// Commit-time operations, with any changes, committed as one transaction in one frame, on a stream of its own
/// (`PROTOCOL.md` §6.7): the commit step decides the operations in commit order and the Commit reply reports each
/// outcome. The command header and sections 0 to 5 are [`crate::DirectWrite`]'s, section 0 may be empty, and the args
/// hold the change records' and then each operation's in order.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the change records, 16 bytes each, in order |
/// | 1 | the args, u32s |
/// | 2 | the term table's records |
/// | 3 | the term table's text |
/// | 4 | the names: ends, empty |
/// | 5 | the names: text, empty |
/// | 6 | the operations, 24 bytes each, the layout of [`crate::Operation`]'s command header bytes 0 to 23, at least one |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalWrite {
    header: WriteHeader,
    terms: TermTable,
    changes: Vec<WriteChange>,
    operations: Vec<CommitOperation>,
}

impl ConditionalWrite {
    /// A ConditionalWrite of `changes` and `operations` naming `terms` in `tenant`, through no session, holding no
    /// token.
    pub fn new(
        tenant: u64,
        terms: TermTable,
        changes: Vec<WriteChange>,
        operations: Vec<CommitOperation>,
    ) -> Self {
        Self::from_parts(WriteHeader::new(tenant), terms, changes, operations)
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

    /// The commit-time operations, in order.
    pub fn operations(&self) -> &[CommitOperation] {
        &self.operations
    }

    pub(crate) fn header(&self) -> &WriteHeader {
        &self.header
    }

    pub(crate) fn from_parts(
        header: WriteHeader,
        terms: TermTable,
        changes: Vec<WriteChange>,
        operations: Vec<CommitOperation>,
    ) -> Self {
        Self {
            header,
            terms,
            changes,
            operations,
        }
    }
}
