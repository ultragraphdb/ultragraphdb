/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ResultTerm;

use crate::types::graph_slot::GraphSlot;
use crate::types::read_request::ReadRequest;

/// A row's unbound positions, by their terms, each named by the partition that answered it.
#[derive(Debug, Default)]
pub(crate) struct RowCells {
    pub(crate) graph: Option<ResultTerm>,
    pub(crate) subject: Option<ResultTerm>,
    pub(crate) predicate: Option<ResultTerm>,
    pub(crate) object: Option<ResultTerm>,
    pub(crate) reifier: Option<ResultTerm>,
}

impl RowCells {
    /// The cells in the columns of `read`: its unbound slots in slot order, then the reifier.
    pub(crate) fn columns(&self, read: &ReadRequest) -> Vec<Option<&ResultTerm>> {
        let mut cells = Vec::with_capacity(5);
        if read.graph == GraphSlot::Unbound {
            cells.push(self.graph.as_ref());
        }
        if read.subject.is_none() {
            cells.push(self.subject.as_ref());
        }
        if read.predicate.is_none() {
            cells.push(self.predicate.as_ref());
        }
        if read.object.is_none() {
            cells.push(self.object.as_ref());
        }
        cells.push(self.reifier.as_ref());
        cells
    }
}
