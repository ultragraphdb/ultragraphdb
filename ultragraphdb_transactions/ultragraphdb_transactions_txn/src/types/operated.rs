/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::Executed;

use crate::types::prepared::dictionary_of;

/// A transaction as it commits once its commit-time operations have run (7.5): its execution with the operations'
/// writes on top, and its write-set record written again from it, the record the round logs, so that a replay applies
/// what was applied and never runs an operation again.
#[derive(Debug)]
pub struct Operated {
    pub(crate) executed: Executed,
    pub(crate) record: Vec<u8>,
}

impl Operated {
    /// The execution re-run with the operations' results.
    pub fn executed(&self) -> &Executed {
        &self.executed
    }

    /// The record's bytes, in `ultragraphdb_protocol_wire`'s layout.
    pub fn record(&self) -> &[u8] {
        &self.record
    }

    /// The dictionary entries the execution creates, the transaction's and its operations', each a key of the
    /// dictionary keyspace and its value, for its round's dictionary section.
    pub fn dictionary(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        dictionary_of(&self.executed)
    }
}
