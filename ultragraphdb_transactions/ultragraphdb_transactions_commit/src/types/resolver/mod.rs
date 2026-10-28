/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::WriteSetRecordView;
use ultragraphdb_transactions_txn::ReadSet;

use crate::errors::commit_error::CommitError;
use crate::types::conflict_history::ConflictHistory;

/// Exact validation, the rules a transaction of a round is decided by before it takes a version, in this order: a read
/// set handed over unsealed is refused; a read version below the history's floor is too old; a read that a
/// transaction committed above the read version wrote conflicts. The round's transactions admitted before it are in
/// the history at their versions, all above any read version a transaction holds (a read version is at most the
/// watermark, and a round's versions lie above every version decided before it), so one check covers both the writes
/// committed above its read version and the writes of its round admitted before it. An admitted transaction's writes
/// are added after it is admitted ([`Resolver::admit`]); an aborted one adds nothing.
///
/// `QuadStore::confirm`'s new-IRI check is the absent-term rule: an execution that creates an IRI found its
/// dictionary key absent and recorded it, and the commit that writes that dictionary entry first, before it in the
/// round or above its read version, makes it conflict.
#[derive(Debug, Clone)]
pub struct Resolver {
    history: ConflictHistory,
}

impl Resolver {
    /// A resolver over an empty history pruned to `floor`.
    pub fn new(floor: u64) -> Self {
        Self {
            history: ConflictHistory::new(floor),
        }
    }

    /// Decides whether a transaction reading `read_set` may be admitted, changing nothing.
    ///
    /// # Errors
    ///
    /// [`CommitError::Unsealed`], [`CommitError::TooOld`] and [`CommitError::Conflict`], in that order.
    pub fn check(&self, read_set: &ReadSet) -> Result<(), CommitError> {
        if !read_set.is_sealed() {
            return Err(CommitError::Unsealed());
        }
        let floor = self.history.floor();
        if read_set.read_version() < floor {
            return Err(CommitError::TooOld(read_set.read_version(), floor));
        }
        match self.history.conflict(read_set) {
            Some(kind) => Err(CommitError::Conflict(kind)),
            None => Ok(()),
        }
    }

    /// Adds the conflict points, statements and ranges of `record`, admitted at `version`, to the history.
    pub fn admit(&mut self, version: u64, record: &WriteSetRecordView<'_>) {
        // A range's end key is its value; an empty end reaches past the last key.
        self.history.insert(
            version,
            record.conflict_points().iter().map(|e| e.key()),
            record
                .conflict_statements()
                .iter()
                .map(|e| (e.key(), e.value().unwrap_or(&[]))),
            record
                .conflict_ranges()
                .iter()
                .map(|e| (e.key(), e.value().filter(|end| !end.is_empty()))),
        );
    }

    /// Prunes the history below `floor`.
    pub fn prune(&mut self, floor: u64) {
        self.history.prune(floor);
    }

    pub fn history(&self) -> &ConflictHistory {
        &self.history
    }
}
