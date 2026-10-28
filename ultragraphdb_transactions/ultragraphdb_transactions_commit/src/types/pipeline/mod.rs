/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::{ChangesetApplier, Executed};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};

use crate::errors::pipeline_error::PipelineError;
use crate::types::applier_operator::ApplierOperator;
use crate::types::commit_request::CommitRequest;
use crate::types::commit_step::CommitStep;
use crate::types::input::Input;
use crate::types::reply::Reply;
use crate::types::round_bounds::RoundBounds;
use crate::types::sessions::Sessions;
use crate::types::step_output::StepOutput;

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
/// **Commit-time operations** run on the applier ([`ApplierOperator`]), over the store at the watermark and the
/// writes of the round syncing and of the round being decided.
///
/// **Buffers.** The buffers of every transaction the step is done with, applied or released, are kept for the
/// workers ([`Pipeline::buffers_mut`]), so the steady state allocates neither read sets nor record buffers.
#[derive(Debug)]
pub struct Pipeline<'a, L, S, C, E> {
    applier: &'a ChangesetApplier<S, C, E>,
    log: L,
    step: CommitStep<Prepared>,
    operator: ApplierOperator<'a, S, C, E>,
    out: StepOutput<Prepared>,
    replies: Vec<Reply>,
    buffers: Vec<TxnBuffers>,
    unsynced: bool,
}

impl<'a, L, S, C, E> Pipeline<'a, L, S, C, E>
where
    L: OrderedLog,
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor,
{
    /// The pipeline of `log` and `applier`, its commit step after the store's applied version within `bounds`,
    /// holding the session table the store holds.
    ///
    /// # Errors
    ///
    /// [`PipelineError::Sessions`] when the store's session table cannot be read or is corrupted.
    pub fn new(
        applier: &'a ChangesetApplier<S, C, E>,
        log: L,
        bounds: RoundBounds,
    ) -> Result<Self, PipelineError> {
        let applied = applier.quads().lists().applied_ts();
        let sessions = Sessions::load(applier.quads().store()).map_err(PipelineError::Sessions)?;
        Ok(Self {
            applier,
            log,
            step: CommitStep::new(bounds, applied).with_table(sessions),
            operator: ApplierOperator::new(applier),
            out: StepOutput::new(),
            replies: Vec::new(),
            buffers: Vec::new(),
            unsynced: false,
        })
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
    /// [`PipelineError::Log`] when the log refuses the round, which stops the pipeline.
    pub fn commit(
        &mut self,
        ticket: u64,
        prepared: Prepared,
        now_us: u64,
    ) -> Result<(), PipelineError> {
        self.run(Input::Commit(CommitRequest::new(ticket, prepared)), now_us)
    }

    /// Hands a session registration to the commit step under `ticket` at `now_us`; it is answered with the session's
    /// number once its round is applied.
    ///
    /// # Errors
    ///
    /// [`PipelineError::Log`] when the log refuses the round, which stops the pipeline.
    pub fn register(&mut self, ticket: u64, now_us: u64) -> Result<(), PipelineError> {
        self.run(Input::Register(ticket), now_us)
    }

    /// Syncs the round appended and not yet synced, if any, at `now_us`: once durable, it is applied and its replies
    /// handed out, and the next round closes over the requests waiting.
    ///
    /// # Errors
    ///
    /// [`PipelineError::Log`] when the sync fails or the next round is refused, and [`PipelineError::Apply`] when the
    /// store refuses the synced round; each stops the pipeline.
    pub async fn sync(&mut self, now_us: u64) -> Result<(), PipelineError> {
        if !self.unsynced {
            return Ok(());
        }
        self.unsynced = false;
        match self.log.sync().await {
            Ok(_) => self.run(Input::Synced, now_us),
            Err(cause) => {
                self.stop(now_us);
                Err(PipelineError::Log(cause))
            }
        }
    }

    /// The oldest read version an open read-write transaction holds, which prunes the conflict history.
    pub fn oldest_read(&mut self, version: u64) {
        self.step.step(Input::OldestRead(version), 0, &mut self.out);
    }

    /// Steps on `input`, then does what the output asks, in order.
    fn run(&mut self, input: Input<Prepared>, now_us: u64) -> Result<(), PipelineError> {
        self.step
            .step_with(input, now_us, &mut self.out, &mut self.operator);
        let refused = self.append().err();
        let applied = self.apply();
        if applied.is_ok() {
            self.replies.append(&mut self.out.replies);
        }
        self.release();
        match (refused, applied) {
            (_, Err(cause)) => {
                self.stop(now_us);
                Err(cause)
            }
            (Some(cause), Ok(())) => {
                self.stop(now_us);
                Err(PipelineError::Log(cause))
            }
            (None, Ok(())) => Ok(()),
        }
    }

    /// Appends the round the step closed at the position after the log's tail, naming the tail's checksum.
    fn append(&mut self) -> Result<(), ultragraphdb_error_log::LogError> {
        let (tail, parent) = self.log.tail();
        let position = Position::new(tail.get().saturating_add(1));
        let Some(round) = self.out.round_ref(position, parent) else {
            return Ok(());
        };
        self.log.append(round)?;
        self.unsynced = true;
        Ok(())
    }

    /// Applies the synced round, if any committed anything, with the session table's entries it changed and its
    /// last version, keeping its transactions' buffers.
    fn apply(&mut self) -> Result<(), PipelineError> {
        let Some(through) = self.out.applied.and_then(|record| record.last_version()) else {
            return Ok(());
        };
        let mut round: Vec<(u64, Executed)> = Vec::with_capacity(self.out.apply.len());
        for (version, prepared) in self.out.apply.drain(..) {
            let (executed, buffers) = prepared.into_parts();
            round.push((version, executed));
            self.buffers.push(buffers);
        }
        self.applier
            .apply_round_with(round, &self.out.system, through)
            .map(|_| ())
            .map_err(PipelineError::Apply)
    }

    /// Keeps the buffers of the proposals the step released.
    fn release(&mut self) {
        let released = self
            .out
            .released
            .drain(..)
            .map(|(_, prepared)| prepared.into_parts().1);
        self.buffers.extend(released);
    }

    /// Stops the step: the round in flight, the requests waiting and every later commit are answered unavailable.
    fn stop(&mut self, now_us: u64) {
        self.unsynced = false;
        self.step.step(Input::SyncFailed, now_us, &mut self.out);
        self.replies.append(&mut self.out.replies);
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

    pub fn step(&self) -> &CommitStep<Prepared> {
        &self.step
    }

    pub fn log(&self) -> &L {
        &self.log
    }

    /// The log, for a restart's recovery.
    pub fn into_log(self) -> L {
        self.log
    }
}
