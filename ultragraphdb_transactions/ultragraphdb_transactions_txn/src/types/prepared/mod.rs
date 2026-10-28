/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::Executed;

use crate::types::read_set::ReadSet;
use crate::types::txn_buffers::TxnBuffers;

/// A read-write transaction ready for the commit step: its execution, which `apply_round` writes once the transaction
/// is admitted; its read set, sealed, which the resolver checks against the writes committed above its read version;
/// and its write-set record's bytes, which the round's log frame carries unchanged and whose conflict sections the
/// resolver adds to its history once the transaction is admitted. Nothing in it needs the store again.
#[derive(Debug)]
pub struct Prepared {
    executed: Executed,
    read_set: ReadSet,
    record: Vec<u8>,
}

impl Prepared {
    pub(crate) fn new(executed: Executed, read_set: ReadSet, record: Vec<u8>) -> Self {
        Self {
            executed,
            read_set,
            record,
        }
    }

    pub fn namespace(&self) -> u64 {
        self.executed.namespace()
    }

    /// The version the transaction read at.
    pub fn read_version(&self) -> u64 {
        self.executed.read_ts()
    }

    pub fn executed(&self) -> &Executed {
        &self.executed
    }

    /// The read set, sealed.
    pub fn read_set(&self) -> &ReadSet {
        &self.read_set
    }

    /// The write-set record's bytes, in `ultragraphdb_protocol_wire`'s layout.
    pub fn record(&self) -> &[u8] {
        &self.record
    }

    /// The execution, for `apply_round`, and the buffers, for the worker's next transaction.
    pub fn into_parts(self) -> (Executed, TxnBuffers) {
        (
            self.executed,
            TxnBuffers::from_parts(self.read_set, self.record),
        )
    }
}
