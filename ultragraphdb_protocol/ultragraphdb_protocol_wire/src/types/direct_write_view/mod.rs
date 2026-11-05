/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod direct_write_view_body_view;

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::term_table_view::TermTableView;
use crate::types::write_change::WriteChange;
use crate::utils::write_frame::{WriteHeader, WriteSections};

/// The body of a received DirectWrite frame, read in place; see [`crate::DirectWrite`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectWriteView<'a> {
    header: WriteHeader,
    sections: WriteSections<'a>,
}

impl<'a> DirectWriteView<'a> {
    /// The principal the frontend filled; 0 from a client.
    pub fn principal(&self) -> u64 {
        self.header.principal
    }

    /// The rule the client endpoint holds a received write to: `principal` 0, refused as
    /// [`ProtocolError::field_value`] (`"principal"`).
    pub fn check_from_client(&self) -> Result<(), ProtocolError> {
        self.header.check_from_client(Command::DirectWrite.number())
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
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.sections.terms
    }

    /// The number of changes, at least one.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Whether the view holds no change: false for every accepted frame, which holds at least one.
    pub fn is_empty(&self) -> bool {
        self.sections.len() == 0
    }

    /// Change `index`, `None` past the last.
    pub fn change(&self, index: usize) -> Option<WriteChange> {
        self.sections.change(index)
    }
}
