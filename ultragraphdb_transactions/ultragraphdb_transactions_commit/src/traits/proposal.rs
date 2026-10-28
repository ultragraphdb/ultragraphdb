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

    /// Whether the proposal asks for commit-time operations, which the step's [`crate::Operator`] runs once it is
    /// admitted (7.5); none by default.
    fn has_operations(&self) -> bool {
        false
    }

    /// Whether the proposal's declaration starts an index build: one runs at a time, so a second waits in the step,
    /// set aside, until the first's end is admitted (decided 2026-10-28); none by default.
    fn starts_build(&self) -> bool {
        false
    }

    /// Whether the proposal is the end of a build, its swap or undo: a round of its own, after which a proposal set
    /// aside may start the next build (decided 2026-10-28); none by default.
    fn ends_build(&self) -> bool {
        false
    }

    /// Whether the proposal is a drop, a write of its key ranges: its round closes after it, so the drop is applied
    /// once the round's batch is written, at its round's last version (7.7); none by default.
    fn is_drop(&self) -> bool {
        false
    }
}
