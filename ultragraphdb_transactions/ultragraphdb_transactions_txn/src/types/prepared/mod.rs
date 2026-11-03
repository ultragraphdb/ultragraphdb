/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{SessionToken, SystemId};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_protocol_wire::OperationResult;
use ultragraphdb_store_apply::{
    BuildEnd, ChangesetApplier, DropRequest, Executed, Operation, OperationOutcome,
};
use ultragraphdb_store_posting::{Batch, CachedList};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store, VersionlessEntry};

use crate::errors::txn_error::TxnError;
use crate::types::operated::Operated;
use crate::types::read_set::ReadSet;
use crate::types::txn_buffers::TxnBuffers;
use crate::types::write_set_builder::WriteSetBuilder;

/// How a transaction's record is written again once its commit-time operations have run: its session, request number,
/// principal, catalog version, commit record and the builder's bounds. Kept only for a transaction with operations.
#[derive(Debug, Default)]
pub(crate) struct Rewrite {
    pub session: u128,
    pub request: u64,
    pub principal: u64,
    pub catalog: u64,
    pub commit_record: Vec<u8>,
    pub objects_per_list: usize,
    pub max_write_set: usize,
}

/// A read-write transaction ready for the commit step: its execution, which `apply_round` writes once the transaction
/// is admitted; its read set, sealed, which the resolver checks against the writes committed above its read version;
/// its logical write-set record's bytes, which the round's log frame carries unchanged and whose conflict sections the
/// resolver adds to its history once the transaction is admitted; and the session token it carries, which holds it
/// until its epoch may be stamped. Nothing in it needs the store again.
#[derive(Debug)]
pub struct Prepared {
    executed: Executed,
    read_set: ReadSet,
    record: Vec<u8>,
    operations: Vec<Operation>,
    rewrite: Rewrite,
    token: Option<SessionToken>,
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
            token: None,
        }
    }

    /// The proposal carrying `token`, its session's token.
    pub(crate) fn with_token(self, token: Option<SessionToken>) -> Self {
        Self { token, ..self }
    }

    /// The drop `drop` asks in `namespace` on `applier` as a proposal of the commit step, on `buffers` (7.7): request
    /// `request` of session `session`, for `principal`, as a Commit is, so a retry is answered from the session table;
    /// a record naming the drop's target and its dropped key ranges, which abort every transaction that read or wrote in
    /// them below its version and commits after it, and whose round's apply drops what it names; its round closes after
    /// it.
    ///
    /// # Errors
    ///
    /// [`TxnError::changeset`] for the store's error; the record's refusal, which releases what the drop's execution
    /// holds.
    pub fn drop<S, C, E, B>(
        applier: &ChangesetApplier<S, C, E, B>,
        namespace: u64,
        drop: DropRequest<'_>,
        session: u128,
        request: u64,
        principal: u64,
        buffers: TxnBuffers,
    ) -> Result<Self, TxnError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let executed = applier.prepare_drop(namespace, drop)?;
        let (mut read_set, mut record) = buffers.into_parts();
        read_set.reset(namespace, executed.read_ts(), usize::MAX);
        read_set.seal();
        let written = WriteSetBuilder::new(&executed, session, request, &[])
            .with_principal(principal)
            .write_into(&mut record, usize::MAX);
        if let Err(refused) = written {
            applier.release(&executed);
            return Err(refused);
        }
        Ok(Self::new(
            executed,
            read_set,
            record,
            Vec::new(),
            Rewrite::default(),
        ))
    }

    /// The end of a build sequenced through the commit step on `applier` as a proposal of the step, on `buffers`
    /// (decided 2026-10-28): a sealed empty read set at the watermark and a record naming the build's end, which the
    /// step decides in a round of its own and whose round's apply writes at the version the step gives it.
    ///
    /// # Errors
    ///
    /// [`TxnError::changeset`] for the store's error; the record's refusal, which releases what the end's execution
    /// holds.
    pub fn end_build<S, C, E, B>(
        applier: &ChangesetApplier<S, C, E, B>,
        end: BuildEnd,
        buffers: TxnBuffers,
    ) -> Result<Self, TxnError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let executed = applier.prepare_build_end(end)?;
        let (mut read_set, mut record) = buffers.into_parts();
        read_set.reset(executed.namespace(), executed.read_ts(), usize::MAX);
        read_set.seal();
        let written =
            WriteSetBuilder::new(&executed, 0, 0, &[]).write_into(&mut record, usize::MAX);
        if let Err(refused) = written {
            applier.release(&executed);
            return Err(refused);
        }
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

    /// The session token the transaction carries, `None` for none.
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    /// The dictionary entries the transaction's execution creates, each a key of the dictionary keyspace and its value,
    /// for its round's dictionary section: never in its record, and conflicting with nothing.
    pub fn dictionary(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        dictionary_of(&self.executed)
    }

    /// Runs the transaction's operations on `applier`, on top of its writes, against the store at the watermark with
    /// `overlay`'s layers above it and `dictionary`, the dictionary entries the unapplied writes made, read as its own, as
    /// the commit step does once the transaction is admitted: the transaction as it commits, its execution and its
    /// record written again with the operations' writes and their results, a node named by its IRI or skolem IRI, which
    /// [`Prepared::adopt`] takes; each operation's result appended to `results`. The transaction is left as it was.
    ///
    /// # Errors
    ///
    /// [`TxnError::changeset`] for an operation refused, a compare-and-set that did not hold above all, or the store's
    /// error; [`TxnError::write_set_too_large`] when the record written again is past the transaction's limit. Each
    /// releases the dictionary holds the operations took, once, and leaves the transaction's own with it.
    pub fn operated<S, C, E, B>(
        &self,
        applier: &ChangesetApplier<S, C, E, B>,
        overlay: &Batch<'_, S, C>,
        dictionary: &[&VersionlessEntry],
        results: &mut Vec<OperationResult>,
    ) -> Result<Operated, TxnError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let mut outcomes = Vec::new();
        let executed = applier.operate(
            &self.executed,
            &self.operations,
            overlay,
            dictionary,
            &mut outcomes,
        )?;
        match self.rewritten(applier, &executed, &outcomes, results) {
            Ok(record) => Ok(Operated { executed, record }),
            Err(refused) => {
                applier.release_operated(&executed, &self.executed);
                Err(refused)
            }
        }
    }

    /// The record of `executed`, the transaction's execution with its operations' writes, written again with their
    /// `outcomes`, which are appended to `results` as the record names them.
    ///
    /// # Errors
    ///
    /// [`TxnError::changeset`] for the store's error naming a node; [`TxnError::write_set_too_large`] past the
    /// transaction's limit.
    fn rewritten<S, C, E, B>(
        &self,
        applier: &ChangesetApplier<S, C, E, B>,
        executed: &Executed,
        outcomes: &[OperationOutcome],
        results: &mut Vec<OperationResult>,
    ) -> Result<Vec<u8>, TxnError>
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let mut record = WriteSetBuilder::new(
            executed,
            self.rewrite.session,
            self.rewrite.request,
            &self.rewrite.commit_record,
        )
        .with_principal(self.rewrite.principal)
        .with_catalog(self.rewrite.catalog)
        .with_objects_per_list(self.rewrite.objects_per_list)
        .build()?;
        for outcome in outcomes {
            match *outcome {
                OperationOutcome::Node { node, created } => {
                    let name = node_name(applier, executed, node)?;
                    record.push_node_result(&name, created);
                }
                OperationOutcome::Set => record.push_result(OperationResult::compare_and_set()),
                OperationOutcome::Counter(value) => {
                    record.push_result(OperationResult::counter(value))
                }
            }
        }
        results.extend_from_slice(record.results());
        let mut bytes = Vec::new();
        let _record_len: usize =
            WriteSetBuilder::encode_into(&record, &mut bytes, self.rewrite.max_write_set)?;
        Ok(bytes)
    }

    /// Takes `operated`, what [`Prepared::operated`] gave for this transaction, as its execution and record: what the
    /// commit step applies and logs. The execution replaced is not released: the operated one carries its terms and
    /// their holds, so releasing both would let go of a hold twice.
    pub fn adopt(&mut self, operated: Operated) {
        self.executed = operated.executed;
        self.record = operated.record;
    }

    /// Releases `operated`, what [`Prepared::operated`] gave for this transaction, when the commit step does not take
    /// it (deferred, refused or no longer fitting its round), writing nothing: the dictionary holds its operations took
    /// beyond the transaction's own are let go, once, and the transaction keeps its own, which its own release or
    /// apply ends. An `operated` taken by [`Prepared::adopt`] is never released this way.
    pub fn release_operated<S, C, E, B>(
        &self,
        applier: &ChangesetApplier<S, C, E, B>,
        operated: Operated,
    ) where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let Operated { executed, .. } = operated;
        applier.release_operated(&executed, &self.executed);
    }

    /// The namespace the transaction ran in.
    pub fn namespace(&self) -> u64 {
        self.executed.namespace()
    }

    /// The version the transaction read at.
    pub fn read_version(&self) -> u64 {
        self.executed.read_ts()
    }

    /// The transaction's execution, which the commit step applies.
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

    /// Releases the proposal on `applier`, the commit step having decided not to apply it, writing nothing: the
    /// dictionary entries its execution created or took while uncommitted are let go, each forgotten once no other
    /// unsettled write holds it (transactions, "An ended transaction releases what it holds"). Its buffers, for the
    /// worker's next transaction. A proposal applied is never released: its round's apply settles what it holds.
    pub fn release<S, C, E, B>(self, applier: &ChangesetApplier<S, C, E, B>) -> TxnBuffers
    where
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let (executed, buffers) = self.into_parts();
        applier.release(&executed);
        buffers
    }

    /// The execution, for `apply_round`, and the buffers, for the worker's next transaction. A proposal the commit
    /// step will not apply is [`Prepared::release`]d instead.
    pub fn into_parts(self) -> (Executed, TxnBuffers) {
        (
            self.executed,
            TxnBuffers::from_parts(self.read_set, self.record),
        )
    }
}

/// The dictionary entries `executed` creates, each a key of the dictionary keyspace and its value.
pub(crate) fn dictionary_of(executed: &Executed) -> impl Iterator<Item = (&[u8], &[u8])> {
    let dictionary = Keyspace::Dictionary.byte();
    executed
        .writes()
        .versionless()
        .iter()
        .filter(move |e| e.key().first() == Some(&dictionary))
        .filter_map(|e| Some((e.key(), e.value()?)))
}

/// The IRI or skolem IRI naming `node`: the execution's own new term, or the dictionary's.
fn node_name<S, C, E, B>(
    applier: &ChangesetApplier<S, C, E, B>,
    executed: &Executed,
    node: SystemId,
) -> Result<String, TxnError>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    if let Some((text, _)) = executed.terms().pending_iri(node) {
        return Ok(text.to_string());
    }
    let (text, _) = applier
        .quads()
        .iri_of(executed.namespace(), node)
        .map_err(|e| TxnError::changeset(e.into()))?;
    Ok(text)
}
