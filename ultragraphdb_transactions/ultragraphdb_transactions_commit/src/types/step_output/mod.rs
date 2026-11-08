/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_core_ids::Epoch;
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
/// Beside them, **close request** ([`StepOutput::close_request`]): the epoch to ask the sequencer to close now, when
/// the open epoch filled in this step and its commits are held for the close.
///
/// Beside them, **released** ([`StepOutput::released`]): every proposal the step will not apply, under its ticket, in
/// the step that decides so (an abort when its round closes, a refusal when it arrives, the round in flight and the
/// requests waiting when a sync fails), so the caller gives each one's buffers back to its worker as a committed
/// one's come back from the apply (approved 2026-10-28).
///
/// A home's registrations ([`crate::Input::Registration`]) carry no proposal, so the caller keeps what each one's round
/// applies: **registered** ([`StepOutput::registered`]) names the applied round's registrations under their tickets at
/// their commit versions, in round order, to apply with it; **dismissed** ([`StepOutput::dismissed`]) the tickets of
/// the registrations the step will not apply, in the step that decides so, to release.
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
    pub(crate) registered: Vec<(u64, u64)>,
    pub(crate) dismissed: Vec<u64>,
    pub(crate) close_request: Option<Epoch>,
    pub(crate) position: u64,
}

impl<P> Default for StepOutput<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> StepOutput<P> {
    /// An empty output.
    pub fn new() -> Self {
        Self {
            append: None,
            body: Vec::new(),
            applied: None,
            apply: Vec::new(),
            system: Vec::new(),
            replies: Vec::new(),
            released: Vec::new(),
            registered: Vec::new(),
            dismissed: Vec::new(),
            close_request: None,
            position: 0,
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
        self.registered.clear();
        self.dismissed.clear();
        self.close_request = None;
        self.position = 0;
    }

    /// The round closed by this step, its record and its Round body, `None` when none closed.
    pub fn append(&self) -> Option<(RoundRecord, &[u8])> {
        self.append.map(|record| (record, self.body.as_slice()))
    }

    /// The log position of the round closed by this step, which its replies' session tokens name: the one after the
    /// log's tail; `None` when none closed.
    #[must_use]
    pub fn position(&self) -> Option<Position> {
        self.append.map(|_| Position::new(self.position))
    }

    /// The round to append as the log takes it, at its position ([`StepOutput::position`]) after the round whose
    /// checksum is `parent`, through the derivation version `derivation`.
    #[must_use]
    pub fn round_ref(&self, parent: u64, derivation: NonZeroU32) -> Option<RoundRef<'_>> {
        let position = self.position()?;
        self.append.map(|record| {
            RoundRef::new(
                position,
                parent,
                record.first_version(),
                record.transactions(),
                derivation,
                &self.body,
            )
        })
    }

    /// The epoch to ask the sequencer to close now: the open epoch, which filled in this step, its commits held until
    /// the close is delivered; `None` otherwise.
    #[must_use]
    pub fn close_request(&self) -> Option<Epoch> {
        self.close_request
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

    /// The applied round's registrations, each its ticket and commit version, in round order: what each one's home
    /// left applies at that version with the round.
    #[must_use]
    pub fn registered(&self) -> &[(u64, u64)] {
        &self.registered
    }

    /// The tickets of the registrations this step will not apply: refused, or dropped with a round that failed.
    #[must_use]
    pub fn dismissed(&self) -> &[u64] {
        &self.dismissed
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
