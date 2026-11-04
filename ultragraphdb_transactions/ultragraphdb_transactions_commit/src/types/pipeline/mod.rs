/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_cluster_placement::{PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::{CatalogVersion, Close};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_apply::LogicalQuad;
use ultragraphdb_store_apply::{ChangesetApplier, ChangesetError, Executed};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_log::{DurabilityTicket, OrderedLog};
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};

use crate::errors::commit_error::CommitError;
use crate::errors::pipeline_error::PipelineError;
use crate::types::applier_operator::ApplierOperator;
use crate::types::close_request::CloseRequest;
use crate::types::commit_request::CommitRequest;
use crate::types::commit_step::CommitStep;
use crate::types::input::Input;
use crate::types::reply::Reply;
use crate::types::round_bounds::RoundBounds;
use crate::types::sessions::Sessions;
use crate::types::step_output::StepOutput;
use crate::types::stored_reply::StoredReply;

/// The derivation version of the rounds the pipeline appends, the store's.
const DERIVATION: std::num::NonZeroU32 = std::num::NonZeroU32::MIN;

const _: () = assert!(LogicalQuad::DERIVATION == DERIVATION.get());

/// The commit step's pipeline on one node: the [`CommitStep`] over real transactions, the ordered log its rounds are
/// appended to and synced in, and the changeset applier each synced round is applied by, driving every
/// [`StepOutput`] in its order: the round closed is appended at the log's tail, the round synced is applied with
/// `apply_round_with`, with the session table's entries it changed and its last version, which moves the visibility
/// watermark past it, and then its replies are handed out. Round k + 1 is decided and appended while round k's sync is
/// pending, and applied once that sync has completed and k is applied.
///
/// **Sessions** (7.8) are written with the rounds that change them, in the same batch, so the store holds the table
/// as of the last round it applied, and a pipeline opened over the store loads it back.
///
/// **Stopping.** A sync that fails, or a round the log refuses, acknowledges none of its round and stops the step:
/// its transactions, the requests waiting and every later commit are answered unavailable, and the error is returned
/// once. A synced round the store refuses is durable but not applied: its replies are withheld, the step stops the
/// same way, and recovery replays it after a restart.
///
/// **Builds** (decided 2026-10-28). The pipeline sequences its applier's builds through the commit step, so every
/// write takes its version from the step: a build done, failed or cancelled ends with a proposal
/// ([`Pipeline::poll_builds`]), its swap or undo, decided in a round of its own under [`Pipeline::SYSTEM_TICKET`],
/// whose reply goes to nobody.
///
/// **Commit-time operations** run on the applier ([`ApplierOperator`]), over the store at the watermark and the
/// writes of the round syncing and of the round being decided.
///
/// **Release.** Every proposal the step will not apply, an abort, a refusal or a proposal the step stopped on, is
/// released on the applier as the step lets it go, so the dictionary entries it created or took are let go; a proposal
/// applied is settled by its round's apply instead (transactions, "An ended transaction releases what it holds").
///
/// **Buffers.** The buffers of every transaction the step is done with, applied or released, are kept for the
/// workers ([`Pipeline::buffers_mut`]), so the steady state allocates neither read sets nor record buffers.
#[derive(Debug)]
pub struct Pipeline<'a, L, S, C, E, B> {
    applier: &'a ChangesetApplier<S, C, E, B>,
    log: L,
    step: CommitStep<Prepared>,
    operator: ApplierOperator<'a, S, C, E, B>,
    out: StepOutput<Prepared>,
    replies: Vec<Reply>,
    buffers: Vec<TxnBuffers>,
    unsynced: bool,
    /// The close the step asked the sequencer for, not yet handed out.
    close_request: Option<Epoch>,
    /// What each registration handed to the step and not yet applied or released leaves, under its ticket, in the order
    /// handed over.
    registrations: VecDeque<(u64, Executed)>,
}

impl<'a, L, S, C, E, B> Pipeline<'a, L, S, C, E, B>
where
    L: OrderedLog,
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The ticket of the proposals the pipeline makes itself, a build's end: a caller's ticket is never this one.
    pub const SYSTEM_TICKET: u64 = u64::MAX;

    /// The pipeline of `log` and `applier`, its commit step after the store's applied version within `bounds`,
    /// holding the session table the store holds, and the applier's builds sequenced through the step from now on.
    ///
    /// # Errors
    ///
    /// [`PipelineError::sessions`] when the store's session table cannot be read or is corrupted.
    pub fn new(
        applier: &'a ChangesetApplier<S, C, E, B>,
        log: L,
        bounds: RoundBounds,
    ) -> Result<Self, PipelineError> {
        let applied = applier.quads().lists().applied_ts();
        let sessions = Sessions::load(applier.quads().store()).map_err(PipelineError::sessions)?;
        let (tail, _) = log.tail();
        applier.sequence_builds();
        Ok(Self {
            applier,
            step: CommitStep::new(bounds, applied)
                .with_table(sessions)
                .with_build_running(applier.rebuilding())
                .with_tail(tail.get()),
            log,
            operator: ApplierOperator::new(applier),
            out: StepOutput::new(),
            replies: Vec::new(),
            buffers: Vec::new(),
            unsynced: false,
            close_request: None,
            registrations: VecDeque::new(),
        })
    }

    /// The pipeline whose commit-time operations claim for `partition`, which owns the buckets `placement` gives it: a
    /// claim of a value whose bucket another partition owns refuses its transaction cross-partition
    /// ([`ApplierOperator::bound`]).
    pub fn bound(self, partition: PartitionId, placement: &'a PlacementMap) -> Self {
        Self {
            operator: self.operator.bound(partition, placement),
            ..self
        }
    }

    /// The pipeline with a commit queue of at most `limit` requests waiting.
    pub fn with_queue(self, limit: usize) -> Self {
        Self {
            step: self.step.with_queue(limit),
            ..self
        }
    }

    /// The pipeline with a session table of at most `capacity` sessions.
    pub fn with_sessions(self, capacity: usize) -> Self {
        Self {
            step: self.step.with_sessions(capacity),
            ..self
        }
    }

    /// Hands `prepared` to the commit step under `ticket` at `now_us`; a round it closes is appended to the log.
    ///
    /// # Errors
    ///
    /// [`PipelineError::log`] when the log refuses the round, which stops the pipeline.
    pub fn commit(
        &mut self,
        ticket: u64,
        prepared: Prepared,
        now_us: u64,
    ) -> Result<(), PipelineError> {
        self.run(Input::Commit(CommitRequest::new(ticket, prepared)), now_us)
    }

    /// The pipeline deciding for partition `partition`, its latest delivered close `delivered`, waiting
    /// `close_request` for a close it asks for, and holding `catalog`, the catalog versions in effect at its open epoch.
    pub fn with_partition(
        self,
        partition: u32,
        delivered: Epoch,
        close_request: CloseRequest,
        catalog: &[CatalogVersion],
    ) -> Self {
        Self {
            step: self
                .step
                .with_partition(partition)
                .with_delivered(delivered)
                .with_close_request(close_request)
                .with_catalog(catalog),
            ..self
        }
    }

    /// Hands a registration the partition accepted as an entity's home to the commit step under `ticket` at `now_us`:
    /// `executed`, what [`ultragraphdb_store_apply::Registering::finish`] left, and `record`, its write-set record of kind
    /// 2. Its round applies `executed` at the registration's own commit version, and it is answered with that version once
    /// the round is applied; a registration the step refuses releases `executed`, writing nothing.
    ///
    /// # Errors
    ///
    /// [`PipelineError::log`] when the log refuses the round, which stops the pipeline.
    pub fn registration(
        &mut self,
        ticket: u64,
        executed: Executed,
        record: Vec<u8>,
        now_us: u64,
    ) -> Result<(), PipelineError> {
        let dictionary = Keyspace::Dictionary.byte();
        let entries = executed
            .writes()
            .versionless()
            .iter()
            .filter(|e| e.key().first() == Some(&dictionary))
            .filter_map(|e| Some((e.key().to_vec(), e.value()?.to_vec())))
            .collect();
        self.registrations.push_back((ticket, executed));
        self.run(
            Input::Registration {
                ticket,
                record,
                dictionary: entries,
            },
            now_us,
        )
    }

    /// Hands the commit step `close`, a close of the sequencer delivered to the partition, at `now_us`: the commits
    /// held for it are decided in a round of the new epoch, which is appended to the log.
    ///
    /// # Errors
    ///
    /// [`PipelineError::log`] when the log refuses the round, which stops the pipeline.
    pub fn close(&mut self, close: Close, now_us: u64) -> Result<(), PipelineError> {
        self.run(Input::Close(close), now_us)
    }

    /// Steps the commit step's clock to `now_us`: a close request unanswered for `close_request_ms` refuses the held
    /// commits busy.
    pub fn tick(&mut self, now_us: u64) {
        // A tick refuses what waits for a close; it closes no round, so nothing is appended or applied.
        self.step
            .step_with(Input::Tick, now_us, &mut self.out, &mut self.operator);
        let replies = self.out.replies.drain(..);
        self.replies
            .extend(replies.filter(|r| r.ticket() != Self::SYSTEM_TICKET));
        self.release();
    }

    /// The close the commit step asked the sequencer for since the last call, its open epoch full, for the caller to
    /// send.
    pub fn take_close_request(&mut self) -> Option<Epoch> {
        self.close_request.take()
    }

    /// A retry of `request` of `session` with no transaction open for it, answered from the session table
    /// ([`CommitStep::retry`]): the stored reply.
    ///
    /// # Errors
    ///
    /// As [`CommitStep::retry`] refuses.
    pub fn retry(&self, session: u128, request: u64) -> Result<StoredReply, CommitError> {
        self.step.retry(session, request)
    }

    /// Syncs the round appended and not yet synced, if any, at `now_us`: once durable, it is applied and its replies
    /// handed out, and the next round closes over the requests waiting.
    ///
    /// # Errors
    ///
    /// [`PipelineError::log`] when the sync fails or the next round is refused, and [`PipelineError::apply`] when the
    /// store refuses the synced round; each stops the pipeline.
    pub async fn sync(&mut self, now_us: u64) -> Result<(), PipelineError> {
        if !self.unsynced {
            return Ok(());
        }
        self.unsynced = false;
        match self.log.sync().await {
            Ok(_) => {
                self.run(Input::Synced, now_us)?;
                self.poll_builds(now_us)
            }
            Err(cause) => {
                self.stop(now_us);
                Err(PipelineError::log(cause))
            }
        }
    }

    /// Proposes the end of every build the applier finished since the last call at `now_us`, its swap or undo, each a
    /// round of its own; [`Pipeline::sync`] calls it too. The node calls it when its executor finishes a build.
    ///
    /// # Errors
    ///
    /// [`PipelineError::build_end`] when an end cannot be proposed, and [`PipelineError::log`] when the log refuses its
    /// round; each stops the pipeline.
    pub fn poll_builds(&mut self, now_us: u64) -> Result<(), PipelineError> {
        // Each turn takes one build the applier finished, of the few it runs at once.
        while let Some(end) = self.applier.take_build_end() {
            let buffers = self.buffers.pop().unwrap_or_default();
            let prepared = match Prepared::end_build(self.applier, end, buffers) {
                Ok(prepared) => prepared,
                Err(cause) => {
                    self.stop(now_us);
                    return Err(PipelineError::build_end(cause));
                }
            };
            self.run(
                Input::Commit(CommitRequest::new(Self::SYSTEM_TICKET, prepared)),
                now_us,
            )?;
        }
        Ok(())
    }

    /// Makes catalog version `contents` take effect in the partition, under `ticket`, at `now_us`: the round installing
    /// it ([`Prepared::catalog`]) is proposed to the commit step, which decides it as a build's start, and its reply
    /// comes with the commits' under `ticket`; a conflict, a commit having changed what its checks read, is answered so,
    /// and the host proposes the version again. The declarations the partition's data refuses are returned, for the
    /// host to report to the metadata group, which restores them by a new version. The host calls it once the close
    /// announcing the version is delivered, before the commits of its epoch.
    ///
    /// # Errors
    ///
    /// [`PipelineError::catalog_version`] when a build runs, the version waiting for its end, leaving the pipeline as it
    /// was; or when the store fails executing the version, which stops the pipeline; the pipeline's errors stepping.
    pub fn install(
        &mut self,
        ticket: u64,
        contents: &CatalogContents,
        now_us: u64,
    ) -> Result<Vec<ChangesetError>, PipelineError> {
        let buffers = self.buffers.pop().unwrap_or_default();
        let (prepared, refusals) = match Prepared::catalog(self.applier, contents, buffers) {
            Ok(prepared) => prepared,
            Err(cause) => {
                if !self.applier.rebuilding() {
                    self.stop(now_us);
                }
                return Err(PipelineError::catalog_version(contents.version(), cause));
            }
        };
        self.run(Input::Commit(CommitRequest::new(ticket, prepared)), now_us)?;
        Ok(refusals)
    }

    /// The oldest read version an open read-write transaction holds, which prunes the conflict history.
    pub fn oldest_read(&mut self, version: u64) {
        self.step.step(Input::OldestRead(version), 0, &mut self.out);
    }

    /// Steps on `input`, then does what the output asks, in order.
    fn run(&mut self, input: Input<Prepared>, now_us: u64) -> Result<(), PipelineError> {
        self.step
            .step_with(input, now_us, &mut self.out, &mut self.operator);
        if let Some(epoch) = self.out.close_request() {
            self.close_request = Some(epoch);
        }
        let refused = self.append().err();
        let applied = self.apply();
        if applied.is_ok() {
            let replies = self.out.replies.drain(..);
            self.replies
                .extend(replies.filter(|r| r.ticket() != Self::SYSTEM_TICKET));
        }
        self.release();
        match (refused, applied) {
            (_, Err(cause)) => {
                self.stop(now_us);
                Err(cause)
            }
            (Some(cause), Ok(())) => {
                self.stop(now_us);
                Err(PipelineError::log(cause))
            }
            (None, Ok(())) => Ok(()),
        }
    }

    /// Appends the round the step closed at the position after the log's tail, naming the tail's checksum.
    fn append(&mut self) -> Result<(), ultragraphdb_error_log::LogError> {
        let (tail, parent) = self.log.tail();
        let Some(round) = self.out.round_ref(parent, DERIVATION) else {
            return Ok(());
        };
        debug_assert_eq!(round.position().get(), tail.get().saturating_add(1));
        let _ticket: DurabilityTicket = self.log.append(round)?;
        self.unsynced = true;
        Ok(())
    }

    /// Applies the synced round, if any committed anything, with the session table's entries it changed and its
    /// last version, keeping its transactions' buffers.
    fn apply(&mut self) -> Result<(), PipelineError> {
        let Some(record) = self.out.applied else {
            return Ok(());
        };
        // A round of aborts alone moves nothing but the applied position, which its batch records.
        let through = record
            .last_version()
            .unwrap_or_else(|| self.applier.quads().lists().applied_ts());
        let mut round: Vec<(u64, Executed)> =
            Vec::with_capacity(self.out.apply.len() + self.out.registered.len());
        for (version, prepared) in self.out.apply.drain(..) {
            let (executed, buffers) = prepared.into_parts();
            round.push((version, executed));
            self.buffers.push(buffers);
        }
        // A registration's home writes apply at the registration's own version, among the round's commits.
        for (ticket, version) in self.out.registered.drain(..) {
            if let Some(executed) = take_registration(&mut self.registrations, ticket) {
                round.push((version, executed));
            }
        }
        round.sort_by_key(|(version, _)| *version);
        self.applier
            .apply_round_with(round, &self.out.system, through)
            .map(|_| ())
            .map_err(PipelineError::apply)
    }

    /// Releases the proposals the step will not apply on the applier, each letting go of the dictionary entries it
    /// holds (transactions, "An ended transaction releases what it holds"), and keeps their buffers.
    fn release(&mut self) {
        let applier = self.applier;
        let released = self
            .out
            .released
            .drain(..)
            .map(|(_, prepared)| prepared.release(applier));
        self.buffers.extend(released);
        for ticket in self.out.dismissed.drain(..) {
            if let Some(executed) = take_registration(&mut self.registrations, ticket) {
                applier.release(&executed);
            }
        }
    }

    /// Stops the step: the round in flight, the requests waiting and every later commit are answered unavailable.
    fn stop(&mut self, now_us: u64) {
        self.unsynced = false;
        self.step.step(Input::SyncFailed, now_us, &mut self.out);
        let replies = self.out.replies.drain(..);
        self.replies
            .extend(replies.filter(|r| r.ticket() != Self::SYSTEM_TICKET));
        self.release();
    }

    /// The replies handed out, for the caller to drain and send.
    pub fn replies_mut(&mut self) -> &mut Vec<Reply> {
        &mut self.replies
    }

    /// The buffers of the transactions the step is done with, for the caller to drain and lend to its workers.
    pub fn buffers_mut(&mut self) -> &mut Vec<TxnBuffers> {
        &mut self.buffers
    }

    /// Whether a round is appended and waits for its sync.
    pub fn needs_sync(&self) -> bool {
        self.unsynced
    }

    /// Whether the pipeline stopped after a failed sync, a refused round or a refused apply.
    pub fn is_stopped(&self) -> bool {
        self.step.is_stopped()
    }

    /// The visibility watermark: the newest version the store applied, which transactions begin at. A round's last
    /// version can be a registration's, which numbers a session and writes its entry alone.
    pub fn watermark(&self) -> u64 {
        self.applier.quads().lists().applied_ts()
    }

    /// The latest close delivered to the partition, which a home's registration reply names.
    pub fn delivered(&self) -> Epoch {
        self.step.delivered()
    }

    /// The commit step the pipeline drives.
    pub fn step(&self) -> &CommitStep<Prepared> {
        &self.step
    }

    /// The log the pipeline appends to.
    pub fn log(&self) -> &L {
        &self.log
    }

    /// The log, for a restart's recovery.
    pub fn into_log(self) -> L {
        self.log
    }
}

/// Takes what the oldest registration handed over under `ticket` leaves out of `pending`: the step decides
/// registrations in the order they arrive, so the oldest under a ticket is the one decided.
fn take_registration(pending: &mut VecDeque<(u64, Executed)>, ticket: u64) -> Option<Executed> {
    let at = pending.iter().position(|(t, _)| *t == ticket)?;
    pending.remove(at).map(|(_, executed)| executed)
}
