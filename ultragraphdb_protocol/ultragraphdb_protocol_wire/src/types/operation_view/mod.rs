/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod operation_view_body_view;

use crate::types::commit_operation::CommitOperation;
use crate::types::term_table_view::TermTableView;
use crate::utils::commit_operation_layout::OperationFields;

/// The body of a received Operation frame, read in place; see [`crate::Operation`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationView<'a> {
    fields: OperationFields,
    args: Vec<u32>,
    terms: TermTableView<'a>,
}

impl<'a> OperationView<'a> {
    /// The operation's kind: 1 insert-if-absent, 2 upsert-and-link, 3 compare-and-set, 4 counter add, 5 delete an
    /// instance, 6 retract an instance.
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.fields.kind
    }

    /// The counter add's delta; zero for every other operation.
    #[must_use]
    pub fn delta(&self) -> i64 {
        self.fields.delta
    }

    /// The number of properties for creation.
    #[must_use]
    pub fn properties(&self) -> u32 {
        self.fields.properties
    }

    /// The number of an upsert-and-link's links.
    #[must_use]
    pub fn links(&self) -> u32 {
        self.fields.links
    }

    /// The term table the operation's indices name.
    #[must_use]
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The operation its args describe.
    #[must_use]
    pub fn operation(&self) -> CommitOperation {
        self.fields.operation(&self.args)
    }
}
