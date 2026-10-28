/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::{Position, RoundRef};
use ultragraphdb_traits_store::VersionlessEntry;

use crate::types::reply::Reply;
use crate::types::round_record::RoundRecord;

/// What one step of the commit step asks of its caller, in the order the caller does it:
///
/// 1. **Append** ([`StepOutput::append`]): the round the step closed, its record and its Round body, which the caller
///    appends to the log ([`StepOutput::round_ref`] at the log's tail) and starts syncing.
/// 2. **Apply** ([`StepOutput::apply`]): the round whose sync completed, its admitted proposals at their commit
///    versions in round order, which the caller writes to the store as one round; the watermark then passes it.
///    With it, **system** ([`StepOutput::system`]): the session table's entries the round changed, which the caller
///    writes in the round's batch (7.8).
/// 3. **Reply** ([`StepOutput::replies`]): the answers to send, those of the applied round after its apply.
///
/// Beside them, **released** ([`StepOutput::released`]): every proposal the step will not apply, under its ticket, in
/// the step that decides so (an abort when its round closes, a refusal when it arrives, the round in flight and the
/// requests waiting when a sync fails), so the caller gives each one's buffers back to its worker as a committed
/// one's come back from the apply (approved 2026-10-28).
///
/// The caller keeps one output for every step: each step clears it first and its buffers keep their capacity, so a
/// steady state allocates neither the body nor the tables. The caller drains [`StepOutput::apply_mut`] and the
/// drained vector goes back to the step for the next round.
#[derive(Debug, Clone)]
pub struct StepOutput<P> {
    pub(crate) append: Option<RoundRecord>,
    pub(crate) body: Vec<u8>,
    pub(crate) applied: Option<RoundRecord>,
    pub(crate) apply: Vec<(u64, P)>,
    pub(crate) system: Vec<VersionlessEntry>,
    pub(crate) replies: Vec<Reply>,
    pub(crate) released: Vec<(u64, P)>,
}

impl<P> Default for StepOutput<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> StepOutput<P> {
    pub fn new() -> Self {
        Self {
            append: None,
            body: Vec::new(),
            applied: None,
            apply: Vec::new(),
            system: Vec::new(),
            replies: Vec::new(),
            released: Vec::new(),
        }
    }

    /// Clears every part, keeping the buffers' capacity.
    pub fn clear(&mut self) {
        self.append = None;
        self.body.clear();
        self.applied = None;
        self.apply.clear();
        self.system.clear();
        self.replies.clear();
        self.released.clear();
    }

    /// The round closed by this step, its record and its Round body, `None` when none closed.
    pub fn append(&self) -> Option<(RoundRecord, &[u8])> {
        self.append.map(|record| (record, self.body.as_slice()))
    }

    /// The round to append as the log takes it, at `position` after the round whose checksum is `parent`.
    pub fn round_ref(&self, position: Position, parent: u64) -> Option<RoundRef<'_>> {
        self.append.map(|record| {
            RoundRef::new(
                position,
                parent,
                record.first_version(),
                record.transactions(),
                &self.body,
            )
        })
    }

    /// The record of the round whose sync completed, to apply; `None` when none did.
    pub fn applied(&self) -> Option<RoundRecord> {
        self.applied
    }

    /// The applied round's admitted proposals at their commit versions, in round order.
    pub fn apply(&self) -> &[(u64, P)] {
        &self.apply
    }

    /// The session table's entries the applied round changed, for its batch: each session registered or committed,
    /// each evicted, then the index when the sessions held changed.
    pub fn system(&self) -> &[VersionlessEntry] {
        &self.system
    }

    /// The same, for the caller to drain.
    pub fn apply_mut(&mut self) -> &mut Vec<(u64, P)> {
        &mut self.apply
    }

    /// The proposals released by this step under their tickets: decided not to be applied.
    pub fn released(&self) -> &[(u64, P)] {
        &self.released
    }

    /// The same, for the caller to drain.
    pub fn released_mut(&mut self) -> &mut Vec<(u64, P)> {
        &mut self.released
    }

    /// The replies to send.
    pub fn replies(&self) -> &[Reply] {
        &self.replies
    }

    /// The capacity of the body buffer and of the apply and reply tables.
    pub fn capacity(&self) -> (usize, usize, usize) {
        (
            self.body.capacity(),
            self.apply.capacity(),
            self.replies.capacity(),
        )
    }
}
