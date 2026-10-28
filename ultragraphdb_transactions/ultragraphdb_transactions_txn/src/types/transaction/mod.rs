/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNodeRef, Term};
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_apply::{ChangesetApplier, Execution, Operation};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::txn_error::TxnError;
use crate::types::mode::Mode;
use crate::types::prepared::{Prepared, Rewrite};
use crate::types::txn_buffers::TxnBuffers;
use crate::types::txn_limits::TxnLimits;
use crate::types::write_set_builder::WriteSetBuilder;

/// How a transaction's commit leaves its worker: a read-only one ends there, never validated, and gives its buffers
/// back; a read-write one goes to the commit step.
///
/// `Prepared` is unboxed: the value is moved once, from the commit to the commit step's queue, and copying it is cheaper
/// than the heap allocation per read-write commit that boxing would add (performance by design, tasks.md).
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Commit {
    ReadOnly(TxnBuffers),
    ReadWrite(Prepared),
}

/// A transaction on a worker. It reads the snapshot at the visibility watermark W when it begins, or at an earlier
/// version it names, never above W; each of its requests reads that snapshot with the transaction's own writes on
/// top, and nothing it writes is visible elsewhere before the commit step applies it. Its writes are changesets,
/// executed when they arrive; its reads are recorded for validation.
///
/// Time is the caller's: every request carries the worker's monotonic clock in microseconds, so a simulation drives
/// the limits without waiting. A transaction past its limit, 10 s for a read-write and 16 s for a read-only one by
/// default, is refused at its next request or its commit with the retryable [`TxnError::PastLimit`], and is ended,
/// having written nothing; so is one whose changeset is refused. Aborting, or dropping it, writes nothing.
#[derive(Debug)]
pub struct Transaction<'a, S, C> {
    execution: Option<Execution<'a, S, C>>,
    mode: Mode,
    namespace: u64,
    read_version: u64,
    began_us: u64,
    limits: TxnLimits,
    buffers: TxnBuffers,
    operations: Vec<Operation>,
}

impl<'a, S, C> Transaction<'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Begins a transaction of `namespace` on `applier` at `now_us`: at the watermark when `read_version` is 0, at
    /// `read_version` otherwise, with `buffers` for its read set and record.
    ///
    /// # Errors
    ///
    /// [`TxnError::ReadAboveWatermark`] at once, never waiting, when `read_version` is above the watermark; the
    /// store's error loading the namespace's catalog.
    pub fn begin<E>(
        applier: &'a ChangesetApplier<S, C, E>,
        namespace: u64,
        mode: Mode,
        read_version: u64,
        now_us: u64,
        limits: TxnLimits,
        buffers: TxnBuffers,
    ) -> Result<Self, TxnError>
    where
        S: Send + Sync + 'static,
        C: Send + Sync + 'static,
        E: Executor,
    {
        let watermark = applier.quads().lists().applied_ts();
        let read_version = match read_version {
            0 => watermark,
            v if v > watermark => return Err(TxnError::ReadAboveWatermark(v, watermark)),
            v => v,
        };
        let execution = applier.begin(namespace, read_version)?;
        Ok(Self {
            execution: Some(execution),
            mode,
            namespace,
            read_version,
            began_us: now_us,
            limits,
            buffers,
            operations: Vec::new(),
        })
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The version the transaction reads at.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// Whether the transaction was ended by a refusal or its limit: every later request is refused with
    /// [`TxnError::Ended`].
    pub fn is_ended(&self) -> bool {
        self.execution.is_none()
    }

    /// Executes `changeset` at `now_us` on top of the transaction's earlier writes.
    ///
    /// # Errors
    ///
    /// [`TxnError::Ended`]; [`TxnError::PastLimit`], ending it; [`TxnError::ReadOnly`] and [`TxnError::Namespace`],
    /// which leave it as it was; [`TxnError::Changeset`] for a changeset refused or a store that failed, ending it.
    pub fn write(&mut self, changeset: &Changeset, now_us: u64) -> Result<(), TxnError> {
        if self.mode == Mode::ReadOnly {
            return Err(TxnError::ReadOnly());
        }
        if changeset.namespace() != self.namespace {
            return Err(TxnError::Namespace(self.namespace, changeset.namespace()));
        }
        let execution = self.live(now_us)?;
        if let Err(e) = execution.apply(changeset) {
            self.execution = None;
            return Err(e.into());
        }
        Ok(())
    }

    /// Asks the commit step to decide `operation` at `now_us`, after the transaction's other checks and its writes, in
    /// the order asked (7.5): its result comes with the commit's reply, and nothing the transaction writes names it.
    ///
    /// # Errors
    ///
    /// [`TxnError::Ended`]; [`TxnError::PastLimit`], ending it; [`TxnError::ReadOnly`], which leaves it as it was.
    pub fn operate(&mut self, operation: Operation, now_us: u64) -> Result<(), TxnError> {
        if self.mode == Mode::ReadOnly {
            return Err(TxnError::ReadOnly());
        }
        self.live(now_us)?;
        self.operations.push(operation);
        Ok(())
    }

    /// The objects of `subject`'s `predicate` in `graph` at `now_us`, as the transaction sees them, into `out`, which
    /// is cleared first; recorded in its read set.
    ///
    /// # Errors
    ///
    /// [`TxnError::Ended`]; [`TxnError::PastLimit`], ending it; [`TxnError::Changeset`] for a store that failed,
    /// ending it.
    pub fn objects_into(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        now_us: u64,
        out: &mut Vec<Term>,
    ) -> Result<(), TxnError> {
        let execution = self.live(now_us)?;
        if let Err(e) = execution.objects_into(graph, subject, predicate, out) {
            self.execution = None;
            return Err(e.into());
        }
        Ok(())
    }

    /// Ends the transaction at `now_us`: a read-only one there, a read-write one as a [`Prepared`] for the commit
    /// step, committed as request `request` of session `session` with `commit_record`.
    ///
    /// # Errors
    ///
    /// [`TxnError::Ended`]; [`TxnError::PastLimit`]; [`TxnError::ReadSetTooLarge`] and
    /// [`TxnError::WriteSetTooLarge`] past the limits. Nothing is written in each case.
    pub fn commit(
        self,
        session: u64,
        request: u64,
        commit_record: &[u8],
        now_us: u64,
    ) -> Result<Commit, TxnError> {
        let limit = self.limit();
        let Some(execution) = self.execution else {
            return Err(TxnError::Ended());
        };
        if now_us.saturating_sub(self.began_us) > limit.as_micros() as u64 {
            return Err(TxnError::PastLimit(limit.as_millis() as u64));
        }
        if self.mode == Mode::ReadOnly {
            return Ok(Commit::ReadOnly(self.buffers));
        }
        let executed = execution.finish();
        let (mut read_set, mut record) = self.buffers.into_parts();
        read_set.reset(
            self.namespace,
            self.read_version,
            self.limits.max_read_set(),
        );
        read_set.gather(executed.reads())?;
        read_set.seal();
        WriteSetBuilder::new(&executed, session, request, commit_record)
            .with_objects_per_list(self.limits.objects_per_list())
            .write_into(&mut record, self.limits.max_write_set())?;
        let rewrite = Rewrite {
            session,
            request,
            commit_record: if self.operations.is_empty() {
                Vec::new()
            } else {
                commit_record.to_vec()
            },
            objects_per_list: self.limits.objects_per_list(),
            max_write_set: self.limits.max_write_set(),
        };
        Ok(Commit::ReadWrite(Prepared::new(
            executed,
            read_set,
            record,
            self.operations,
            rewrite,
        )))
    }

    /// The limit of the transaction's mode.
    fn limit(&self) -> std::time::Duration {
        match self.mode {
            Mode::ReadWrite => self.limits.read_write(),
            Mode::ReadOnly => self.limits.read_only(),
        }
    }

    /// The execution of a transaction neither ended nor past its limit at `now_us`; one past it is ended.
    fn live(&mut self, now_us: u64) -> Result<&mut Execution<'a, S, C>, TxnError> {
        let limit = self.limit();
        if self.execution.is_some()
            && now_us.saturating_sub(self.began_us) > limit.as_micros() as u64
        {
            self.execution = None;
            return Err(TxnError::PastLimit(limit.as_millis() as u64));
        }
        self.execution.as_mut().ok_or_else(TxnError::Ended)
    }

    /// Aborts the transaction, writing nothing: its buffers, for the worker's next transaction.
    pub fn abort(self) -> TxnBuffers {
        self.buffers
    }
}
