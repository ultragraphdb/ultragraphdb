/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod conditional_write_view_body_view;

use ultragraphdb_core_ids::SessionToken;

use crate::types::commit_operation::CommitOperation;
use crate::types::term_table_view::TermTableView;
use crate::types::write_change::WriteChange;
use crate::utils::commit_operation_layout::{OPERATION_FIELDS_SIZE, OperationFields};
use crate::utils::whole_records::record_at;
use crate::utils::write_frame::{WriteHeader, WriteSections};

/// The body of a received ConditionalWrite frame, read in place; see [`crate::ConditionalWrite`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalWriteView<'a> {
    header: WriteHeader,
    sections: WriteSections<'a>,
    operations: &'a [u8],
    starts: Vec<usize>,
}

impl<'a> ConditionalWriteView<'a> {
    /// The client's session; all zero for none.
    pub fn session(&self) -> [u8; 16] {
        self.header.session
    }

    /// The request's number within its session.
    pub fn request(&self) -> u64 {
        self.header.request
    }

    /// The namespace it acts in.
    pub fn namespace(&self) -> u64 {
        self.header.namespace
    }

    /// The session token the client holds; `None` for none.
    pub fn token(&self) -> Option<&SessionToken> {
        self.header.token.as_ref()
    }

    /// The term table.
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.sections.terms
    }

    /// The number of changes.
    pub fn change_count(&self) -> usize {
        self.sections.len()
    }

    /// Change `index`, `None` past the last.
    pub fn change(&self, index: usize) -> Option<WriteChange> {
        self.sections.change(index)
    }

    /// The number of operations, at least one.
    pub fn operation_count(&self) -> usize {
        self.starts.len()
    }

    /// Operation `index`, `None` past the last.
    pub fn operation(&self, index: usize) -> Option<CommitOperation> {
        let fields = record_at::<OPERATION_FIELDS_SIZE>(self.operations, index)?;
        let args = self.sections.args.get(*self.starts.get(index)?..)?;
        Some(OperationFields::read(fields).operation(args))
    }
}
