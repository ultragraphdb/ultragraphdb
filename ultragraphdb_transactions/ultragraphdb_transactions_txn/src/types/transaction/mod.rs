/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, Term};
use ultragraphdb_cluster_placement::{PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Bucket, CanonicalKey, Namespace, SessionToken, TypePrefix, Version};
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_store_apply::{ChangesetApplier, Executed, Execution, Operation};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::txn_error::TxnError;
use crate::types::mode::Mode;
use crate::types::prepared::{Prepared, Rewrite};
use crate::types::read_set::ReadSet;
use crate::types::txn_buffers::TxnBuffers;
use crate::types::txn_limits::TxnLimits;
use crate::types::write_set_builder::WriteSetBuilder;

/// How a transaction's commit leaves its worker: a read-only one ends there, never validated, and gives its buffers
/// back; a read-write one goes to the commit step.
///
/// `Prepared` is unboxed: the value is moved once, from the commit to the commit step's queue, and copying it is cheaper
/// than the heap allocation per read-write commit that boxing would add (performance by design, tasks.md).
#[expect(
    clippy::large_enum_variant,
    reason = "moving `Prepared` once is cheaper than a heap allocation per read-write commit"
)]
#[derive(Debug)]
pub enum Commit {
    /// A read-only transaction, ended on its worker: its buffers, for the worker's next transaction.
    ReadOnly(TxnBuffers),
    /// A read-write transaction's proposal, for the commit step.
    ReadWrite(Prepared),
}

/// A transaction on a worker. It reads the snapshot at the visibility watermark W when it begins, or at an earlier
/// version it names, never above W; each of its requests reads that snapshot with the transaction's own writes on
/// top, and nothing it writes is visible elsewhere before the commit step applies it. Its writes are changesets,
/// executed when they arrive; its reads are recorded for validation. A read-write transaction is bound to one
/// partition ([`Transaction::bind`]): a write of a subject whose bucket another partition owns aborts it,
/// cross-partition (transactions, "Aborts name their reason"). The session token it carries holds its commit until
/// its epoch may be stamped, and the catalog version it executed against is named in its record.
///
/// Time is the caller's: every request carries the worker's monotonic clock in microseconds, so a simulation drives
/// the limits without waiting. A transaction past its limit, 10 s for a read-write and 16 s for a read-only one by
/// default, is refused at its next request or its commit with the retryable [`TxnError::past_limit`], and is ended,
/// having written nothing; so is one whose changeset is refused. Aborting, or dropping it, writes nothing.
///
/// **Release.** A transaction ending without a commit releases what its execution holds on the applier: the dictionary
/// entries it created or took while uncommitted are let go, each forgotten once no other unsettled write holds it
/// (transactions, "An ended transaction releases what it holds"). It releases when it is aborted, ended by a refusal or
/// its limit, refused at its commit, ended as a read-only commit, or dropped; a read-write commit hands its execution to
/// the [`Prepared`], which the commit step releases unless it applies it.
#[derive(Debug)]
pub struct Transaction<'a, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    applier: &'a ChangesetApplier<S, C, E, B>,
    execution: Option<Execution<'a, S, C, B>>,
    mode: Mode,
    namespace: u64,
    read_version: u64,
    began_us: u64,
    limits: TxnLimits,
    buffers: TxnBuffers,
    operations: Vec<Operation>,
    principal: u64,
    catalog: u64,
    token: Option<SessionToken>,
    binding: Option<(PartitionId, &'a PlacementMap)>,
}

impl<'a, S, C, E, B> Transaction<'a, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// Begins a transaction of `namespace` on `applier` at `now_us`: at the watermark when `read_version` is 0, at
    /// `read_version` otherwise, with `buffers` for its read set and record.
    ///
    /// # Errors
    ///
    /// [`TxnError::read_above_watermark`] at once, never waiting, when `read_version` is above the watermark; the
    /// store's error loading the namespace's catalog.
    pub fn begin(
        applier: &'a ChangesetApplier<S, C, E, B>,
        namespace: u64,
        mode: Mode,
        read_version: u64,
        now_us: u64,
        limits: TxnLimits,
        buffers: TxnBuffers,
    ) -> Result<Self, TxnError> {
        let watermark = applier.quads().lists().applied_ts();
        let read_version = match read_version {
            0 => watermark,
            v if v > watermark => return Err(TxnError::read_above_watermark(v, watermark)),
            v => v,
        };
        debug_assert!(read_version <= watermark);
        // The catalog version the execution checks against: the one the partition holds installed now.
        let catalog = applier.catalog(namespace)?.version();
        let execution = applier.begin(namespace, read_version)?;
        Ok(Self {
            applier,
            execution: Some(execution),
            mode,
            namespace,
            read_version,
            began_us: now_us,
            limits,
            buffers,
            operations: Vec::new(),
            principal: 0,
            catalog,
            token: None,
            binding: None,
        })
    }

    /// The transaction executing against `catalog`, its namespace's catalog version, which its record names; by default
    /// the version the partition held installed when the transaction began.
    pub fn with_catalog(mut self, catalog: u64) -> Self {
        self.catalog = catalog;
        self
    }

    /// The transaction carrying `token`, its session's token: its commit is held until the token's epoch may be
    /// stamped.
    pub fn with_token(mut self, token: SessionToken) -> Self {
        self.token = Some(token);
        self
    }

    /// The transaction bound to `partition`, the owner of the buckets `placement` gives it at the transaction's read
    /// epoch: a write of a subject another partition owns aborts it, cross-partition. Unbound, every write is taken.
    pub fn bind(mut self, partition: PartitionId, placement: &'a PlacementMap) -> Self {
        self.binding = Some((partition, placement));
        self
    }

    /// The catalog version the transaction executes against.
    pub fn catalog(&self) -> u64 {
        self.catalog
    }

    /// The session token the transaction carries.
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    /// The transaction acting for `principal`, which its write-set record names; 0, without authentication, by default.
    pub fn with_principal(mut self, principal: u64) -> Self {
        self.principal = principal;
        self
    }

    /// The principal the transaction acts for.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The namespace the transaction runs in.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// Whether the transaction writes.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The version the transaction reads at.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// Whether the transaction was ended by a refusal or its limit: every later request is refused with
    /// [`TxnError::ended`].
    pub fn is_ended(&self) -> bool {
        self.execution.is_none()
    }

    /// Executes `changeset` at `now_us` on top of the transaction's earlier writes.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::read_only`], [`TxnError::namespace`] and
    /// [`TxnError::declaration`] for a changeset holding a declaration, which leave it as it was;
    /// [`TxnError::cross_partition`] for a subject another partition owns, and [`TxnError::changeset`] for a changeset
    /// refused or a store that failed, each ending it.
    pub fn write(&mut self, changeset: &Changeset, now_us: u64) -> Result<(), TxnError> {
        if self.mode == Mode::ReadOnly {
            return Err(TxnError::read_only());
        }
        if changeset.namespace() != self.namespace {
            return Err(TxnError::namespace(self.namespace, changeset.namespace()));
        }
        // Refused before the execution sees the changeset: nothing of it is written, and the transaction goes on.
        if changeset
            .changes()
            .iter()
            .any(|c| matches!(c, Change::Declare(_)))
        {
            return Err(TxnError::declaration());
        }
        let foreign = self.foreign_subject(changeset);
        let execution = self.live(now_us)?;
        if let Some((partition, other)) = foreign {
            self.end();
            return Err(TxnError::cross_partition(partition, other));
        }
        if let Err(e) = execution.apply(changeset) {
            self.end();
            return Err(e.into());
        }
        Ok(())
    }

    /// Asks the commit step to decide `operation` at `now_us`, after the transaction's other checks and its writes, in
    /// the order asked (7.5): its result comes with the commit's reply, and nothing the transaction writes names it.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::read_only`], which leaves it as it was.
    pub fn operate(&mut self, operation: Operation, now_us: u64) -> Result<(), TxnError> {
        if self.mode == Mode::ReadOnly {
            return Err(TxnError::read_only());
        }
        let _execution: &mut Execution<'a, S, C, B> = self.live(now_us)?;
        self.operations.push(operation);
        Ok(())
    }

    /// The objects of `subject`'s `predicate` in `graph` at `now_us`, as the transaction sees them, into `out`, which
    /// is cleared first; recorded in its read set.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::changeset`] for a store that failed,
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
            self.end();
            return Err(e.into());
        }
        Ok(())
    }

    /// Ends the transaction at `now_us`: a read-only one there, a read-write one as a [`Prepared`] for the commit
    /// step, committed as request `request` of session `session` with `commit_record`.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`]; [`TxnError::read_set_too_large`] and
    /// [`TxnError::write_set_too_large`] past the limits; [`TxnError::changeset`] for a store that failed finishing
    /// the execution. Nothing is written in each case, and what the execution held is released, once: by the commit's
    /// refusal, or by [`Execution::finish`] itself when finishing it failed.
    pub fn commit(
        mut self,
        session: u128,
        request: u64,
        commit_record: &[u8],
        now_us: u64,
    ) -> Result<Commit, TxnError> {
        let limit = self.limit();
        if self.execution.is_none() {
            return Err(TxnError::ended());
        }
        if past(now_us.saturating_sub(self.began_us), limit) {
            return Err(TxnError::past_limit(limit.as_millis() as u64));
        }
        if self.mode == Mode::ReadOnly {
            return Ok(Commit::ReadOnly(std::mem::take(&mut self.buffers)));
        }
        let Some(execution) = self.execution.take() else {
            return Err(TxnError::ended());
        };
        let executed = execution.finish()?;
        debug_assert_eq!(executed.read_ts(), self.read_version);
        debug_assert_eq!(executed.namespace(), self.namespace);
        let (mut read_set, mut record) = std::mem::take(&mut self.buffers).into_parts();
        let built = self.build(
            &executed,
            &mut read_set,
            &mut record,
            session,
            request,
            commit_record,
        );
        if let Err(refused) = built {
            self.applier.release(&executed);
            return Err(refused);
        }
        let rewrite = Rewrite {
            session,
            request,
            principal: self.principal,
            catalog: self.catalog,
            commit_record: if self.operations.is_empty() {
                Vec::new()
            } else {
                commit_record.to_vec()
            },
            objects_per_list: self.limits.objects_per_list(),
            max_write_set: self.limits.max_write_set(),
        };
        let operations = std::mem::take(&mut self.operations);
        let prepared = Prepared::new(executed, read_set, record, operations, rewrite);
        Ok(Commit::ReadWrite(prepared.with_token(self.token.take())))
    }

    /// Gathers `executed`'s reads into `read_set`, sealed, and writes its record, request `request` of session
    /// `session` with `commit_record`, into `record`, within the transaction's limits.
    ///
    /// # Errors
    ///
    /// [`TxnError::read_set_too_large`] and [`TxnError::write_set_too_large`] past the limits.
    fn build(
        &self,
        executed: &Executed,
        read_set: &mut ReadSet,
        record: &mut Vec<u8>,
        session: u128,
        request: u64,
        commit_record: &[u8],
    ) -> Result<(), TxnError> {
        read_set.reset(
            self.namespace,
            self.read_version,
            self.limits.max_read_set(),
        );
        read_set.gather(executed.reads())?;
        read_set.seal();
        let _record_len: usize = WriteSetBuilder::new(executed, session, request, commit_record)
            .with_principal(self.principal)
            .with_catalog(self.catalog)
            .with_objects_per_list(self.limits.objects_per_list())
            .write_into(record, self.limits.max_write_set())?;
        Ok(())
    }

    /// Ends the transaction, writing nothing: the execution it still holds, if any, is released on the applier
    /// unfinished, deriving nothing. Every later request is refused with [`TxnError::ended`].
    fn end(&mut self) {
        if let Some(execution) = self.execution.take() {
            self.applier.release_execution(execution);
        }
    }

    /// The limit of the transaction's mode.
    fn limit(&self) -> std::time::Duration {
        match self.mode {
            Mode::ReadWrite => self.limits.read_write(),
            Mode::ReadOnly => self.limits.read_only(),
        }
    }

    /// The execution of a transaction neither ended nor past its limit at `now_us`; one past it is ended.
    fn live(&mut self, now_us: u64) -> Result<&mut Execution<'a, S, C, B>, TxnError> {
        let limit = self.limit();
        if self.execution.is_some() && past(now_us.saturating_sub(self.began_us), limit) {
            self.end();
            return Err(TxnError::past_limit(limit.as_millis() as u64));
        }
        self.execution.as_mut().ok_or_else(TxnError::ended)
    }

    /// The bound partition and the owner of the first subject of `changeset` another partition owns, at the
    /// transaction's read epoch; `None` when unbound or every subject is the partition's own. A blank node's subject is
    /// new or the partition's own: a new one is drawn among the partition's buckets.
    fn foreign_subject(&self, changeset: &Changeset) -> Option<(u32, u32)> {
        let (partition, placement) = self.binding?;
        let epoch = Version::from_u64(self.read_version).epoch();
        let namespace = Namespace::new(self.namespace);
        changeset.changes().iter().find_map(|change| {
            let subject = match change {
                Change::Insert { subject, .. }
                | Change::InsertEdge { subject, .. }
                | Change::Delete { subject, .. }
                | Change::DeletePattern { subject, .. } => subject,
                Change::DeleteStatement { .. }
                | Change::SetProperty { .. }
                | Change::DeleteProperty { .. }
                | Change::Declare(_) => return None,
            };
            let NamedOrBlankNode::NamedNode(iri) = subject else {
                return None;
            };
            let key = CanonicalKey::iri(iri.as_str()).ok()?;
            let owner = placement
                .partition_of(Bucket::derive(namespace, TypePrefix::RESOURCE, &key), epoch);
            (owner != partition).then_some((partition.get(), owner.get()))
        })
    }

    /// Aborts the transaction, writing nothing and releasing what its execution held: its buffers, for the worker's
    /// next transaction.
    pub fn abort(mut self) -> TxnBuffers {
        self.end();
        std::mem::take(&mut self.buffers)
    }
}

impl<S, C, E, B> Drop for Transaction<'_, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// A transaction dropped by its worker or its host releases what its execution still holds, as an abort does; one
    /// committed or aborted holds none.
    fn drop(&mut self) {
        self.end();
    }
}

/// Whether `elapsed_us` microseconds are past `limit`, compared in u128: a limit's microseconds may exceed `u64::MAX`,
/// and cut to u64 they would wrap to a far shorter limit.
fn past(elapsed_us: u64, limit: std::time::Duration) -> bool {
    u128::from(elapsed_us) > limit.as_micros()
}
