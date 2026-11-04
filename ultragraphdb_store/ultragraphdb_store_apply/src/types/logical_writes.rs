/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Graph;

use crate::types::logical_quad::LogicalQuad;
use crate::types::marked_reference::MarkedReference;

/// A logged transaction's writes in their logical form (write-ahead-log, "A round's record is logical"): the statements
/// it inserted, each once per statement ID its posting carries, the statements it removed, a delete by pattern as the
/// statements it resolved to, its statement records, by graph and statement ID, `None` for a removal, and the
/// registrations it relied on as a referrer (write-set record section 4), each written as its mark. Replay derives every
/// key the first apply derived from them ([`crate::ChangesetApplier::prepare_logical`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogicalWrites {
    inserted: Vec<LogicalQuad>,
    removed: Vec<LogicalQuad>,
    records: Vec<(Graph, SystemId, Option<Vec<u8>>)>,
    marks: Vec<MarkedReference>,
}

impl LogicalWrites {
    /// The writes `inserted`, `removed` and `records` hold.
    pub fn new(
        inserted: Vec<LogicalQuad>,
        removed: Vec<LogicalQuad>,
        records: Vec<(Graph, SystemId, Option<Vec<u8>>)>,
    ) -> Self {
        Self {
            inserted,
            removed,
            records,
            marks: Vec::new(),
        }
    }

    /// The writes with section 4's `marks`, in the record's order.
    pub fn with_marks(mut self, marks: Vec<MarkedReference>) -> Self {
        self.marks = marks;
        self
    }

    /// The registrations relied on as a referrer, each written as its mark.
    pub fn marks(&self) -> &[MarkedReference] {
        &self.marks
    }

    /// The statements inserted.
    pub fn inserted(&self) -> &[LogicalQuad] {
        &self.inserted
    }

    /// The statements removed.
    pub fn removed(&self) -> &[LogicalQuad] {
        &self.removed
    }

    /// The statement records written, `None` for a removal.
    pub fn records(&self) -> &[(Graph, SystemId, Option<Vec<u8>>)] {
        &self.records
    }
}
