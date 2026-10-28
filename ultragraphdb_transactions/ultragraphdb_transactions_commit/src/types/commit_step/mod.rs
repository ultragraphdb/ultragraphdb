/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_step_round;

use std::collections::VecDeque;

use ultragraphdb_protocol_wire::OperationResult;

use crate::errors::commit_error::CommitError;
use crate::traits::operator::Operator;
use crate::traits::proposal::Proposal;
use crate::types::commit_request::CommitRequest;
use crate::types::input::Input;
use crate::types::no_operations::NoOperations;
use crate::types::reply::Reply;
use crate::types::resolver::Resolver;
use crate::types::round_bounds::RoundBounds;
use crate::types::round_record::RoundRecord;
use crate::types::sequencer::Sequencer;
use crate::types::sessions::Sessions;
use crate::types::step_output::StepOutput;

/// A request waiting for a round: a commit, or a session registration under its ticket.
#[derive(Debug)]
enum Waiting<P> {
    Commit(CommitRequest<P>),
    Register(u64),
}

/// Where a committed decision's record comes from: the round's admitted proposal or registration of that index.
#[derive(Debug, Clone, Copy)]
enum Source {
    Txn(usize),
    Registration(usize),
}

/// What the table takes once a round is applied: a session registered, or a session's request committed at a
/// version with its operations' results.
#[derive(Debug, Clone)]
enum Update {
    Registered(u64),
    Committed {
        session: u64,
        request: u64,
        version: u64,
        results: Vec<OperationResult>,
    },
}

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
/// **The commit queue** holds the requests waiting, at most [`CommitStep::with_queue`]'s limit: a commit arriving to
/// a full queue is refused busy, retryable, and released.
///
/// **A failed sync** acknowledges none of the round in flight and stops the step: its transactions, the requests
/// waiting and every later request are answered unavailable.
///
/// **Sessions** (D9). A commit names its session and request number in its record's fixed part, session 0 for none.
/// Before anything else is checked, a retry of the session's last committed request is answered with that commit's
/// stored reply, its version and its operations' results, and committed nothing (its proposal released), and a
/// session evicted, busy with a request in flight or numbering out of order is refused; a registration
/// ([`Input::Register`]) takes a commit version in its round, which numbers the session, its record a write-set record
/// of that session, request 0 and no writes. The table takes each round's registrations and committed requests once
/// the round is applied, the oldest session evicted when full, and hands the entries they changed to the round's
/// apply ([`StepOutput::system`]), so the store holds the table as of every round it applied.
///
/// **Builds** (decided 2026-10-28). One index build runs at a time, from the admission of the proposal starting it to
/// the admission of its end, its swap or undo, which is a round of its own: a proposal starting another build meanwhile
/// waits, set aside ([`CommitStep::deferred`]), and goes back to the front of the queue once the end is admitted.
///
/// **Decided but unapplied.** The round in flight keeps its admitted proposals until its sync completes
/// ([`CommitStep::in_flight`]), which commit-time operations read beside the store at the watermark.
#[derive(Debug)]
pub struct CommitStep<P> {
    bounds: RoundBounds,
    sequencer: Sequencer,
    resolver: Resolver,
    waiting: VecDeque<Waiting<P>>,
    /// The proposals starting a build, set aside while one runs.
    deferred: VecDeque<Waiting<P>>,
    /// Whether a build runs: its start admitted, its end not yet.
    building: bool,
    in_flight: Option<RoundRecord>,
    admitted: Vec<(u64, P)>,
    decided: Vec<(u64, Result<u64, CommitError>)>,
    sources: Vec<Source>,
    registrations: Vec<Vec<u8>>,
    updates: Vec<Update>,
    /// The commit-time operations' results of the round's admitted transactions that asked any, under their tickets.
    results: Vec<(u64, Vec<OperationResult>)>,
    sessions: Sessions,
    queue: usize,
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
            deferred: VecDeque::new(),
            building: false,
            in_flight: None,
            admitted: Vec::new(),
            decided: Vec::new(),
            sources: Vec::new(),
            registrations: Vec::new(),
            updates: Vec::new(),
            results: Vec::new(),
            sessions: Sessions::new(Sessions::DEFAULT_CAPACITY),
            queue: usize::MAX,
            watermark: previous,
            stopped: false,
        }
    }

    /// The step with a commit queue of at most `limit` requests waiting, at least one: a commit arriving to a full
    /// queue is refused busy, retryable. Unbounded by default; the node sets it.
    pub fn with_queue(self, limit: usize) -> Self {
        Self {
            queue: limit.max(1),
            ..self
        }
    }

    /// The step with a session table of at most `capacity` sessions, the oldest evicted until it holds no more.
    pub fn with_sessions(self, capacity: usize) -> Self {
        Self {
            sessions: self.sessions.bounded(capacity),
            ..self
        }
    }

    /// The step with a build running when `running` holds, as the store it applies to has one when it opens.
    pub fn with_build_running(self, running: bool) -> Self {
        Self {
            building: running,
            ..self
        }
    }

    /// The step holding `sessions`, the table a store held ([`Sessions::load`]): its pending evictions are written
    /// with the first round applied.
    pub fn with_table(self, sessions: Sessions) -> Self {
        Self { sessions, ..self }
    }

    /// Steps on `input` at `now_us` of the commit step's clock, filling `out`, which it clears first. A proposal asking
    /// for commit-time operations is refused: the step has no operator ([`CommitStep::step_with`]).
    pub fn step(&mut self, input: Input<P>, now_us: u64, out: &mut StepOutput<P>) {
        self.step_with(input, now_us, out, &mut NoOperations);
    }

    /// Steps on `input` as [`CommitStep::step`] does, `operator` running the commit-time operations of the proposals
    /// it decides (7.5): after a proposal is admitted by the resolver and before it takes a version, over the store
    /// and the decided but unapplied proposals' writes. A refused operation refuses its transaction, which takes no
    /// version and adds nothing to the history; the writes and record the operations leave are the transaction's from
    /// then on, what the history holds, the round logs and the apply writes. The caller applies each round handed
    /// over before the next step.
    pub fn step_with<O: Operator<P>>(
        &mut self,
        input: Input<P>,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        out.clear();
        match input {
            Input::Commit(request) => self.arrive(request, now_us, out, operator),
            Input::Register(ticket) => self.register(ticket, now_us, out, operator),
            Input::Synced => self.synced(now_us, out, operator),
            Input::SyncFailed => self.stop(out),
            // No open transaction reads above the watermark.
            Input::OldestRead(version) => self.resolver.prune(version.min(self.watermark)),
        }
    }

    /// A commit request arrives: refused when the step is stopped or the request cannot fit any round, else it waits,
    /// and a round closes at once when none is in flight.
    fn arrive<O: Operator<P>>(
        &mut self,
        request: CommitRequest<P>,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        if self.stopped {
            out.replies.push(Reply::new(
                request.ticket(),
                Err(CommitError::Unavailable()),
            ));
            out.released.push(request.into_parts());
            return;
        }
        let alone = commit_step_round::frame_len(1, 1, request.proposal().record().len());
        let limit = self.bounds.bytes() as usize;
        if alone > limit {
            out.replies.push(Reply::new(
                request.ticket(),
                Err(CommitError::TooLarge(alone, limit)),
            ));
            out.released.push(request.into_parts());
            return;
        }
        if self.waiting.len() + self.deferred.len() >= self.queue {
            out.replies.push(Reply::new(
                request.ticket(),
                Err(CommitError::Busy(self.queue)),
            ));
            out.released.push(request.into_parts());
            return;
        }
        self.waiting.push_back(Waiting::Commit(request));
        if self.in_flight.is_none() {
            self.close(now_us, out, operator);
        }
    }

    /// A session registration arrives: refused when the step is stopped, else it waits as a commit does.
    fn register<O: Operator<P>>(
        &mut self,
        ticket: u64,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        if self.stopped {
            out.replies
                .push(Reply::new(ticket, Err(CommitError::Unavailable())));
            return;
        }
        self.waiting.push_back(Waiting::Register(ticket));
        if self.in_flight.is_none() {
            self.close(now_us, out, operator);
        }
    }

    /// The round in flight is synced: it is handed over to apply with its replies, and the next round closes over the
    /// requests waiting.
    fn synced<O: Operator<P>>(&mut self, now_us: u64, out: &mut StepOutput<P>, operator: &mut O) {
        let Some(record) = self.in_flight.take() else {
            return;
        };
        out.applied = Some(record);
        std::mem::swap(&mut out.apply, &mut self.admitted);
        if let Some(last) = record.last_version() {
            self.watermark = last;
        }
        for update in self.updates.drain(..) {
            match update {
                Update::Registered(session) => {
                    self.sessions.register(session);
                }
                Update::Committed {
                    session,
                    request,
                    version,
                    results,
                } => self.sessions.committed(session, request, version, results),
            }
        }
        self.sessions.drain_entries(&mut out.system);
        let results = &mut self.results;
        out.replies
            .extend(self.decided.drain(..).map(|(ticket, outcome)| {
                let reply = Reply::new(ticket, outcome);
                match results.iter().position(|(t, _)| *t == ticket) {
                    Some(at) => reply.with_results(results.swap_remove(at).1),
                    None => reply,
                }
            }));
        results.clear();
        if !self.waiting.is_empty() {
            self.close(now_us, out, operator);
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

    /// The number of proposals starting a build set aside while one runs.
    pub fn deferred(&self) -> usize {
        self.deferred.len()
    }

    /// Whether a build runs: its start admitted and its end not yet.
    pub fn is_building(&self) -> bool {
        self.building
    }

    /// The session table, as the rounds applied left it.
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
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
