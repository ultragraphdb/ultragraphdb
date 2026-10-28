/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_txn::ReadSet;

/// What the commit step decides a read-write transaction by: its read set, sealed, which the resolver checks against
/// the history, and its write-set record's bytes, in `ultragraphdb_protocol_wire`'s `WriteSetRecord` layout, whose
/// conflict sections the resolver adds to the history once the transaction is admitted and which the round's body
/// carries unchanged. A worker's `Prepared` is one; the simulator and the tests hand over their own.
pub trait Proposal {
    /// The read set, sealed: its namespace, its read version and its tables in key order.
    fn read_set(&self) -> &ReadSet;

    /// The write-set record's bytes.
    fn record(&self) -> &[u8];
}
