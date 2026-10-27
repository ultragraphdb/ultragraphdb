/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod operation_body_builder;

use crate::types::commit_operation::CommitOperation;
use crate::types::term_table::TermTable;

/// A commit-time operation (D4), decided by the commit step in commit order; its result comes with the Commit reply.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8: 1 insert-if-absent, 2 upsert-and-link, 3 compare-and-set, 4 counter add |
/// | 8 | `delta` | i64, the counter add's; zero otherwise |
/// | 16 | `properties` | u32, the properties for creation; zero for 3 and 4 |
/// | 20 | `links` | u32, an upsert-and-link's links; zero otherwise |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | args, u32 term indices: graph or [`crate::ABSENT`], then for 1 and 2 the unique predicate, the value, each property and its value, and for 2 each link's predicate, other node and direction (0: the node is the subject, 1: the object); for 3 the subject, predicate, expected and new value; for 4 the subject and predicate |
/// | 1 | the term table's records |
/// | 2 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    terms: TermTable,
    operation: CommitOperation,
}

impl Operation {
    pub fn new(terms: TermTable, operation: CommitOperation) -> Self {
        Self { terms, operation }
    }

    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    pub fn operation(&self) -> &CommitOperation {
        &self.operation
    }
}
