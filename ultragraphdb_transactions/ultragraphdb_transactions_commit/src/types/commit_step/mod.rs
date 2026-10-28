/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_step_round;

use std::collections::VecDeque;

use crate::errors::commit_error::CommitError;
use crate::traits::proposal::Proposal;
use crate::types::commit_request::CommitRequest;
use crate::types::input::Input;
use crate::types::reply::Reply;
use crate::types::resolver::Resolver;
use crate::types::round_bounds::RoundBounds;
use crate::types::round_record::RoundRecord;
use crate::types::sequencer::Sequencer;
use crate::types::step_output::StepOutput;

/// The commit step as a step function: commit requests, sync completions and the oldest open read in
/// ([`Input`]), the round to append with its record and body, the round to apply and the replies out
/// ([`StepOutput`]). One round is in flight at a time, between its close and its sync's completion.
///
/// **Rounds.** A round closes when the sync of the round in flight completes, or at once when a commit request
/// arrives with no round in flight, with no timer. It holds the requests waiting then, in arrival order, while its
/// frame stays within the bounds' bytes, every request counted as committed, and its transactions within their
/// count; the rest wait for the next round. A request whose round alone would exceed the bytes is refused as too large
/// when it arrives. The round's transactions are decided in round order ([`Resolver`]), each admitted one taking the
/// next version of the [`Sequencer`], whose clock is read once per round; the body, every decision and the record of
/// each committed transaction, is written into the output's buffer in `ultragraphdb_protocol_wire`'s `RoundBody`
/// layout. A round is acknowledged as a unit: its replies, aborts included, go out once its sync has completed and
/// its apply is handed over.
///
/// **A failed sync** acknowledges none of the round in flight and stops the step: its transactions, the requests
/// waiting and every later request are answered unavailable.
///
/// **Decided but unapplied.** The round in flight keeps its admitted proposals until its sync completes
/// ([`CommitStep::in_flight`]), which commit-time operations read beside the store at the watermark.
#[derive(Debug)]
pub struct CommitStep<P> {
    bounds: RoundBounds,
    sequencer: Sequencer,
    resolver: Resolver,
    waiting: VecDeque<CommitRequest<P>>,
    in_flight: Option<RoundRecord>,
    admitted: Vec<(u64, P)>,
    decided: Vec<(u64, Result<u64, CommitError>)>,
    watermark: u64,
    stopped: bool,
}

impl<P: Proposal> CommitStep<P> {
    /// A commit step after `previous`, the store's applied version: the watermark, the version the next commits
    /// follow and the history's floor.
    pub fn new(bounds: RoundBounds, previous: u64) -> Self {
        Self {
            bounds,
            sequencer: Sequencer::new(previous),
            resolver: Resolver::new(previous),
            waiting: VecDeque::new(),
            in_flight: None,
            admitted: Vec::new(),
            decided: Vec::new(),
            watermark: previous,
            stopped: false,
        }
    }

    /// Steps on `input` at `now_us` of the commit step's clock, filling `out`, which it clears first.
    pub fn step(&mut self, input: Input<P>, now_us: u64, out: &mut StepOutput<P>) {
        out.clear();
        match input {
            Input::Commit(request) => self.arrive(request, now_us, out),
            Input::Synced => self.synced(now_us, out),
            Input::SyncFailed => self.stop(out),
            // No open transaction reads above the watermark.
            Input::OldestRead(version) => self.resolver.prune(version.min(self.watermark)),
        }
    }

    /// A commit request arrives: refused when the step is stopped or the request cannot fit any round, else it waits,
    /// and a round closes at once when none is in flight.
    fn arrive(&mut self, request: CommitRequest<P>, now_us: u64, out: &mut StepOutput<P>) {
        if self.stopped {
            out.replies.push(Reply::new(
                request.ticket(),
                Err(CommitError::Unavailable()),
            ));
            return;
        }
        let alone = commit_step_round::frame_len(1, 1, request.proposal().record().len());
        let limit = self.bounds.bytes() as usize;
        if alone > limit {
            out.replies.push(Reply::new(
                request.ticket(),
                Err(CommitError::TooLarge(alone, limit)),
            ));
            return;
        }
        self.waiting.push_back(request);
        if self.in_flight.is_none() {
            self.close(now_us, out);
        }
    }

    /// The round in flight is synced: it is handed over to apply with its replies, and the next round closes over the
    /// requests waiting.
    fn synced(&mut self, now_us: u64, out: &mut StepOutput<P>) {
        let Some(record) = self.in_flight.take() else {
            return;
        };
        out.applied = Some(record);
        std::mem::swap(&mut out.apply, &mut self.admitted);
        if let Some(last) = record.last_version() {
            self.watermark = last;
        }
        out.replies.extend(
            self.decided
                .drain(..)
                .map(|(ticket, outcome)| Reply::new(ticket, outcome)),
        );
        if !self.waiting.is_empty() {
            self.close(now_us, out);
        }
    }

    /// The newest version handed over to apply: the visibility watermark once the caller has applied it.
    pub fn watermark(&self) -> u64 {
        self.watermark
    }

    /// The round in flight, closed and not yet synced.
    pub fn in_flight(&self) -> Option<RoundRecord> {
        self.in_flight
    }

    /// The round in flight's admitted proposals at their versions: decided, not yet applied.
    pub fn in_flight_admitted(&self) -> &[(u64, P)] {
        &self.admitted
    }

    /// The number of requests waiting for the next round.
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Whether a failed sync or exhausted versions stopped the step.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    pub fn resolver(&self) -> &Resolver {
        &self.resolver
    }

    pub fn bounds(&self) -> RoundBounds {
        self.bounds
    }

    /// The newest commit version given.
    pub fn previous_version(&self) -> u64 {
        self.sequencer.previous()
    }
}
