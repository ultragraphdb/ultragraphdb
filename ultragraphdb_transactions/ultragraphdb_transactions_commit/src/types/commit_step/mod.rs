/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_step_round;

pub(crate) use commit_step_round::frame_len;

use std::collections::{BTreeMap, VecDeque};

use ultragraphdb_cluster_sequencer::CatalogVersion;
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_protocol_wire::{OperationResult, TermTable, WriteSetRecordView};

use crate::errors::commit_error::CommitError;
use crate::traits::operator::Operator;
use crate::traits::proposal::Proposal;
use crate::types::close_request::CloseRequest;
use crate::types::commit_request::CommitRequest;
use crate::types::epoch_stamp::EpochStamp;
use crate::types::input::Input;
use crate::types::no_operations::NoOperations;
use crate::types::resolver::Resolver;
use crate::types::round_bounds::RoundBounds;
use crate::types::round_record::RoundRecord;
use crate::types::sessions::Sessions;
use crate::types::step_output::StepOutput;
use crate::types::stored_reply::StoredReply;

/// A request waiting for a round: a commit, or a registration a home accepted, under its ticket with its record.
#[derive(Debug)]
pub(crate) enum Waiting<P> {
    Commit(CommitRequest<P>),
    Registration {
        ticket: u64,
        record: Vec<u8>,
        dictionary: Vec<(Vec<u8>, Vec<u8>)>,
    },
}

/// Where a committed decision's record comes from: the round's admitted proposal or registration of that index.
#[derive(Debug, Clone, Copy)]
enum Source {
    Txn(usize),
    Registration(usize),
}

/// What the table takes once a round is applied: a session's request committed at a version with its operations'
/// results; the stored reply's token is the round's.
#[derive(Debug, Clone)]
struct Update {
    session: u128,
    request: u64,
    version: u64,
    results: Vec<OperationResult>,
    terms: TermTable,
}

/// The commit step as a step function: commit requests, sync completions and the oldest open read in
/// ([`Input`]), the round to append with its record and body, the round to apply and the replies out
/// ([`StepOutput`]). One round is in flight at a time, between its close and its sync's completion.
///
/// **Rounds.** A round closes when the sync of the round in flight completes, or at once when a commit request arrives
/// with no round in flight, with no timer. It holds the requests waiting then, in arrival order, while its frame stays
/// within the bounds' bytes, every request counted as committed, and its transactions within their count; the rest wait
/// for the next round. A request whose record is over the largest a round holds on its own
/// ([`RoundBounds::max_write_set`]) is refused as too large when it arrives; one whose commit-time operations leave
/// such a write set, their record and dictionary entries together, or a Commit reply whose frame would pass the floor
/// of `max_frame`, is refused as too large when it is decided, taking no version, and one whose operations leave a
/// write set that a fresh round holds but its own round has no room for waits for the next round, decided again there
/// first. The round's transactions are decided in round order ([`Resolver`]), each admitted one taking the next version
/// of the [`ultragraphdb_cluster_sequencer::Sequencer`], whose clock is read once per round; the body, every decision
/// and the record of each committed transaction, is written into the output's buffer in `ultragraphdb_protocol_wire`'s
/// `RoundBody` layout. A round is acknowledged as a unit: its replies, aborts included, go out once its sync has
/// completed and its apply is handed over.
///
/// **The commit queue** holds the requests waiting, at most [`CommitStep::with_queue`]'s limit: a commit arriving to
/// a full queue is refused busy, retryable, and released.
///
/// **A failed sync** acknowledges none of the round in flight and stops the step: its transactions, the requests
/// waiting and every later request are answered unavailable.
///
/// **Sessions** (D9). A commit names its session and request number in its record's fixed part, session 0 for none. Its
/// session is checked once its record reads and before validation, in this order: one with a request decided and not
/// yet applied is refused as busy; then a Commit on an open transaction numbered at or below the session's last is
/// refused as out of order, naming the last, and aborts ([`Sessions::check`]), while a one-frame or Drop resend of the
/// last is answered with that commit's stored reply, its version and its operations' results, and commits nothing (its
/// proposal released), and one below the last is refused as out of order ([`Sessions::check_one_frame`]). A resend sent
/// while a request of its session is in flight is therefore refused busy, not answered. A registration
/// ([`Input::Registration`]) takes a commit version in its round, which numbers the session, its record a write-set record
/// of that session, request 0 and no writes. The table takes each round's registrations and committed requests once the
/// round is applied, the oldest session evicted when full, and hands the entries they changed to the round's apply
/// ([`StepOutput::system`]), so the store holds the table as of every round it applied.
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
    stamp: EpochStamp,
    /// The partition the step decides for, which session tokens name.
    partition: u32,
    /// The log's tail: the position of the last round closed.
    position: u64,
    /// How long a close request may go unanswered before held and new commits are refused busy.
    close_request: CloseRequest,
    /// When the close of the full open epoch was asked for, on the step's clock; `None` when none is asked.
    asked_us: Option<u64>,
    /// The commits held for a close until their session token's epoch is open. With `waiting` and `deferred` it holds
    /// at most `queue` entries, past which a commit is refused busy.
    held: VecDeque<CommitRequest<P>>,
    /// Since when commits have been held with no close delivered, on the step's clock; `None` when none is held.
    held_us: Option<u64>,
    /// Each tenant's catalog version in effect at the open epoch, by tenant.
    catalog: Vec<(u64, u64)>,
    /// The reason a close out of order stopped the step.
    fault: Option<CommitError>,
    /// The invariant whose violation stopped the step.
    violated: Option<InvariantError>,
    resolver: Resolver,
    waiting: VecDeque<Waiting<P>>,
    /// The proposals starting a build, set aside while one runs.
    deferred: VecDeque<Waiting<P>>,
    /// Whether a build runs: its start admitted, its end not yet.
    building: bool,
    in_flight: Option<RoundRecord>,
    /// The log position of the round in flight, which its commits' session tokens name.
    in_flight_position: u64,
    admitted: Vec<(u64, P)>,
    decided: Vec<(u64, Result<u64, CommitError>)>,
    sources: Vec<Source>,
    registrations: Vec<Vec<u8>>,
    /// The dictionary entries the round's admitted transactions create, its dictionary section.
    dictionary: BTreeMap<Vec<u8>, Vec<u8>>,
    updates: Vec<Update>,
    /// The tickets of the round's admitted transactions through no session, in decision order: their replies hold no
    /// token.
    sessionless: Vec<u64>,
    /// The commit-time operations' results of the round's admitted transactions that asked any, under their tickets,
    /// with the term table naming their nodes.
    results: Vec<(u64, Vec<OperationResult>, TermTable)>,
    sessions: Sessions,
    queue: usize,
    watermark: u64,
    stopped: bool,
}

impl<P: Proposal> CommitStep<P> {
    /// A commit step after `previous`, the store's applied version: the watermark, the newest version stamped and the
    /// history's floor; its latest delivered close the epoch before `previous`'s, or epoch 0 for version 0, until
    /// [`CommitStep::with_delivered`] names it.
    pub fn new(bounds: RoundBounds, previous: u64) -> Self {
        let last = Version::from_u64(previous);
        let delivered = Epoch::new(last.epoch().get().saturating_sub(1)).unwrap_or(Epoch::ZERO);
        Self {
            bounds,
            stamp: EpochStamp::after(delivered, last),
            partition: 0,
            position: 0,
            close_request: CloseRequest::default(),
            asked_us: None,
            held: VecDeque::new(),
            held_us: None,
            catalog: Vec::new(),
            fault: None,
            violated: None,
            resolver: Resolver::new(previous),
            waiting: VecDeque::new(),
            deferred: VecDeque::new(),
            building: false,
            in_flight: None,
            in_flight_position: 0,
            admitted: Vec::new(),
            decided: Vec::new(),
            sources: Vec::new(),
            registrations: Vec::new(),
            dictionary: BTreeMap::new(),
            updates: Vec::new(),
            sessionless: Vec::new(),
            results: Vec::new(),
            sessions: Sessions::new(Sessions::DEFAULT_CAPACITY),
            queue: usize::MAX,
            watermark: previous,
            stopped: false,
        }
    }

    /// The step with `delivered` the latest close delivered to its partition, as the partition's subscription stands
    /// when it opens.
    #[must_use]
    pub fn with_delivered(self, delivered: Epoch) -> Self {
        let last = self
            .stamp
            .last()
            .unwrap_or(Version::from_u64(self.watermark));
        Self {
            stamp: EpochStamp::after(delivered, last),
            ..self
        }
    }

    /// The step deciding for partition `partition`, which its session tokens name; 0 by default.
    #[must_use]
    pub fn with_partition(self, partition: u32) -> Self {
        Self { partition, ..self }
    }

    /// The step whose log's tail is at `tail`, so its next round takes position `tail + 1`; 0 by default.
    #[must_use]
    pub fn with_tail(self, tail: u64) -> Self {
        Self {
            position: tail,
            ..self
        }
    }

    /// The step waiting `close_request` for a close it asked for before refusing held and new commits busy; 60,000 ms
    /// by default.
    #[must_use]
    pub fn with_close_request(self, close_request: CloseRequest) -> Self {
        Self {
            close_request,
            ..self
        }
    }

    /// The step holding `versions`, each tenant's catalog version in effect at its open epoch, as the metadata
    /// group's versions stand when the partition opens; a tenant not named is at version 0.
    #[must_use]
    pub fn with_catalog(self, versions: &[CatalogVersion]) -> Self {
        let mut catalog: Vec<(u64, u64)> = versions
            .iter()
            .map(|v| (v.tenant().get(), v.version()))
            .collect();
        catalog.sort_unstable();
        catalog.dedup_by_key(|(tenant, _)| *tenant);
        Self { catalog, ..self }
    }

    /// The step with a commit queue of at most `limit` requests waiting, at least one: a commit arriving to a full
    /// queue is refused busy, retryable. Unbounded by default; the node sets it.
    pub fn with_queue(self, limit: usize) -> Self {
        Self {
            queue: limit.max(1),
            ..self
        }
    }

    /// The step with a session table of at most `capacity` sessions, the registry's `sessions_max`.
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

    /// The step holding `sessions`, the table a store held ([`Sessions::load`]).
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
            Input::Commit(request) => self.arrive(Waiting::Commit(request), now_us, out, operator),
            Input::Registration {
                ticket,
                record,
                dictionary,
            } => {
                self.arrive(
                    Waiting::Registration {
                        ticket,
                        record,
                        dictionary,
                    },
                    now_us,
                    out,
                    operator,
                );
            }
            Input::Close(close) => self.deliver(&close, now_us, out, operator),
            Input::Tick => self.expire(now_us, out),
            Input::Synced => self.synced(now_us, out, operator),
            Input::SyncFailed => self.stop(out),
            // No open transaction reads above the watermark.
            Input::OldestRead(version) => self.resolver.prune(version.min(self.watermark)),
        }
    }

    /// A retry of `request` of `session`, sent with no transaction open for it: answered from the session table, as
    /// [`Sessions::retry`] answers it, without a round; session busy while a request of the session waits in the step.
    ///
    /// # Errors
    ///
    /// [`CommitError::session_busy`] while a request of the session waits, is held or is decided and not yet applied;
    /// otherwise as [`Sessions::retry`] refuses.
    pub fn retry(&self, session: u128, request: u64) -> Result<StoredReply, CommitError> {
        let in_step = |w: &Waiting<P>| match w {
            Waiting::Commit(c) => session_of(c.proposal().record()) == Some((session, request)),
            Waiting::Registration { .. } => false,
        };
        let held = self
            .held
            .iter()
            .any(|c| session_of(c.proposal().record()) == Some((session, request)));
        if held || self.waiting.iter().chain(self.deferred.iter()).any(in_step) {
            return Err(CommitError::session_busy(session));
        }
        self.sessions.retry(session, request).cloned()
    }

    /// The newest version handed over to apply: the visibility watermark once the caller has applied it.
    #[must_use]
    pub fn watermark(&self) -> u64 {
        self.watermark
    }

    /// The latest close delivered to the partition.
    #[must_use]
    pub fn delivered(&self) -> Epoch {
        self.stamp.delivered()
    }

    /// The partition's open epoch, one above its latest delivered close; `None` past the last epoch.
    #[must_use]
    pub fn open(&self) -> Option<Epoch> {
        self.stamp.open()
    }

    /// The newest epoch the partition has closed: its close delivered and every round stamped at or below it applied.
    /// A snapshot read at an epoch waits until the partition has closed it.
    #[must_use]
    pub fn closed(&self) -> Epoch {
        let delivered = self.stamp.delivered();
        match self.in_flight {
            Some(round) if Version::from_u64(round.first_version()).epoch() <= delivered => {
                Epoch::new(delivered.get().saturating_sub(1)).unwrap_or(Epoch::ZERO)
            }
            Some(_) | None => delivered,
        }
    }

    /// The catalog version of `tenant` in effect at the open epoch, 0 when none is.
    pub fn catalog_version(&self, tenant: u64) -> u64 {
        self.catalog
            .iter()
            .find(|(n, _)| *n == tenant)
            .map_or(0, |(_, version)| *version)
    }

    /// The log's tail: the position of the last round closed.
    #[must_use]
    pub fn tail(&self) -> u64 {
        self.position
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

    /// The number of commits held for a close.
    #[must_use]
    pub fn held(&self) -> usize {
        let full = if self.stamp.is_full() {
            self.waiting.len()
        } else {
            0
        };
        self.held.len() + full
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

    /// Whether a failed sync, exhausted versions, a close out of order or an invariant violated stopped the step.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// The invariant whose violation stopped the step; `None` otherwise.
    #[must_use]
    pub fn violated(&self) -> Option<InvariantError> {
        self.violated
    }

    /// The close out of order that stopped the step, as the reason it names; `None` otherwise.
    #[must_use]
    pub fn fault(&self) -> Option<&CommitError> {
        self.fault.as_ref()
    }

    /// The resolver validating the step's transactions.
    pub fn resolver(&self) -> &Resolver {
        &self.resolver
    }

    /// The bounds a round closes at.
    pub fn bounds(&self) -> RoundBounds {
        self.bounds
    }

    /// The newest commit version given.
    pub fn previous_version(&self) -> u64 {
        self.stamp.last().map_or(self.watermark, Version::to_u64)
    }
}

/// The session and request number of the write-set record `record`, `None` for one that does not read or names no
/// session.
fn session_of(record: &[u8]) -> Option<(u128, u64)> {
    let view = WriteSetRecordView::new(record).ok()?;
    let session = u128::from_be_bytes(view.session());
    (session != 0).then_some((session, view.request()))
}
