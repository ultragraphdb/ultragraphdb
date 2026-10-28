/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::{BuildEnd, ChangesetApplier, Executed, Operation};
use ultragraphdb_store_posting::{Batch, CachedList};
use ultragraphdb_traits_store::{Executor, ListCache, Store, VersionlessEntry};

use crate::errors::txn_error::TxnError;
use crate::types::operated::Operated;
use crate::types::read_set::ReadSet;
use crate::types::txn_buffers::TxnBuffers;
use crate::types::write_set_builder::WriteSetBuilder;

/// How a transaction's record is written again once its commit-time operations have run: its session, request number,
/// commit record and the builder's bounds. Kept only for a transaction with operations.
#[derive(Debug, Default)]
pub(crate) struct Rewrite {
    pub session: u64,
    pub request: u64,
    pub commit_record: Vec<u8>,
    pub objects_per_list: usize,
    pub max_write_set: usize,
}

/// A read-write transaction ready for the commit step: its execution, which `apply_round` writes once the transaction
/// is admitted; its read set, sealed, which the resolver checks against the writes committed above its read version;
/// and its write-set record's bytes, which the round's log frame carries unchanged and whose conflict sections the
/// resolver adds to its history once the transaction is admitted. Nothing in it needs the store again.
#[derive(Debug)]
pub struct Prepared {
    executed: Executed,
    read_set: ReadSet,
    record: Vec<u8>,
    operations: Vec<Operation>,
    rewrite: Rewrite,
}

impl Prepared {
    pub(crate) fn new(
        executed: Executed,
        read_set: ReadSet,
        record: Vec<u8>,
        operations: Vec<Operation>,
        rewrite: Rewrite,
    ) -> Self {
        Self {
            executed,
            read_set,
            record,
            operations,
            rewrite,
        }
    }

    /// The drop of `predicate` of `namespace` on `applier` as a proposal of the commit step, on `buffers` (7.7): a write
    /// of the predicate's key ranges at the watermark, which aborts every transaction that read or wrote in them below
    /// its version and commits after it, and which the round's apply drops; its round closes after it.
    ///
    /// # Errors
    ///
    /// [`TxnError::Changeset`] for the store's error.
    pub fn drop_predicate<S, C, E>(
        applier: &ChangesetApplier<S, C, E>,
        namespace: u64,
        predicate: oxrdf::NamedNodeRef<'_>,
        buffers: TxnBuffers,
    ) -> Result<Self, TxnError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        E: Executor,
    {
        let executed = applier.prepare_drop_predicate(namespace, predicate)?;
        let (mut read_set, mut record) = buffers.into_parts();
        read_set.reset(namespace, executed.read_ts(), usize::MAX);
        read_set.seal();
        WriteSetBuilder::new(&executed, 0, 0, &[]).write_into(&mut record, usize::MAX)?;
        Ok(Self::new(
            executed,
            read_set,
            record,
            Vec::new(),
            Rewrite::default(),
        ))
    }

    /// The end of a build sequenced through the commit step on `applier` as a proposal of the step, on `buffers`
    /// (decided 2026-10-28): a sealed empty read set at the watermark and a record naming the swap or undo, which the
    /// step decides in a round of its own and whose round's apply writes at the version the step gives it.
    ///
    /// # Errors
    ///
    /// [`TxnError::Changeset`] for the store's error.
    pub fn end_build<S, C, E>(
        applier: &ChangesetApplier<S, C, E>,
        end: BuildEnd,
        buffers: TxnBuffers,
    ) -> Result<Self, TxnError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        E: Executor,
    {
        let executed = applier.prepare_build_end(end)?;
        let (mut read_set, mut record) = buffers.into_parts();
        read_set.reset(executed.namespace(), executed.read_ts(), usize::MAX);
        read_set.seal();
        WriteSetBuilder::new(&executed, 0, 0, &[]).write_into(&mut record, usize::MAX)?;
        Ok(Self::new(
            executed,
            read_set,
            record,
            Vec::new(),
            Rewrite::default(),
        ))
    }

    /// The end of a build the proposal is, `None` for any other: its round holds nothing else.
    pub fn build_end(&self) -> Option<BuildEnd> {
        self.executed.build_end()
    }

    /// Whether the proposal is a drop: its round closes after it.
    pub fn is_drop(&self) -> bool {
        self.executed.is_drop()
    }

    /// Whether the transaction's declaration starts an index build: a round holds at most one.
    pub fn starts_build(&self) -> bool {
        self.executed.starts_build()
    }

    /// The commit-time operations the transaction asks the commit step to decide, in order (7.5).
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Whether the transaction asks for commit-time operations.
    pub fn has_operations(&self) -> bool {
        !self.operations.is_empty()
    }

    /// Runs the transaction's operations on `applier`, on top of its writes, against the store at the watermark with
    /// `overlay`'s layers above it and `dictionary`, the dictionary entries the unapplied writes made, read as its own, as
    /// the commit step does once the transaction is admitted: the transaction as it
    /// commits, its execution and its record written again with the operations' writes, which [`Prepared::adopt`]
    /// takes; each operation's result appended to `results`. The transaction is left as it was.
    ///
    /// # Errors
    ///
    /// [`TxnError::Changeset`] for an operation refused, a compare-and-set that did not hold above all, or the store's
    /// error; [`TxnError::WriteSetTooLarge`] when the record written again is past the transaction's limit.
    pub fn operated<S, C, E>(
        &self,
        applier: &ChangesetApplier<S, C, E>,
        overlay: &Batch<'_, S, C>,
        dictionary: &[&VersionlessEntry],
        results: &mut Vec<(u64, bool)>,
    ) -> Result<Operated, TxnError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        E: Executor,
    {
        let executed = applier.operate(
            &self.executed,
            &self.operations,
            overlay,
            dictionary,
            results,
        )?;
        let mut record = Vec::new();
        WriteSetBuilder::new(
            &executed,
            self.rewrite.session,
            self.rewrite.request,
            &self.rewrite.commit_record,
        )
        .with_objects_per_list(self.rewrite.objects_per_list)
        .with_results(results)
        .write_into(&mut record, self.rewrite.max_write_set)?;
        Ok(Operated { executed, record })
    }

    /// Takes `operated`, what [`Prepared::operated`] gave for this transaction, as its execution and record: what the
    /// commit step applies and logs.
    pub fn adopt(&mut self, operated: Operated) {
        self.executed = operated.executed;
        self.record = operated.record;
    }

    pub fn namespace(&self) -> u64 {
        self.executed.namespace()
    }

    /// The version the transaction read at.
    pub fn read_version(&self) -> u64 {
        self.executed.read_ts()
    }

    pub fn executed(&self) -> &Executed {
        &self.executed
    }

    /// The read set, sealed.
    pub fn read_set(&self) -> &ReadSet {
        &self.read_set
    }

    /// The write-set record's bytes, in `ultragraphdb_protocol_wire`'s layout.
    pub fn record(&self) -> &[u8] {
        &self.record
    }

    /// The execution, for `apply_round`, and the buffers, for the worker's next transaction.
    pub fn into_parts(self) -> (Executed, TxnBuffers) {
        (
            self.executed,
            TxnBuffers::from_parts(self.read_set, self.record),
        )
    }
}
