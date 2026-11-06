/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::{Epoch, SessionToken, Version};
use ultragraphdb_core_mapping::Changes;
use ultragraphdb_store_apply::{ChangesetError, DropRequest, Operation};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::is_skolem;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_log::OrderedLog;
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_commit::StoredReply;
use ultragraphdb_transactions_txn::{Commit, Mode, Prepared, TxnError};

use crate::errors::partition_error::PartitionError;
use crate::types::answer::Answer;
use crate::types::bound_transaction::{BoundKey, BoundStep, BoundTransaction};
use crate::types::partition_host::{PartitionHost, RESEND_US};
use crate::types::reference::Reference;

impl<'a, L, S, C, E, B> PartitionHost<'a, L, S, C, E, B>
where
    L: OrderedLog,
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// Opens the read-write transaction of a Bind as `key` at `now_us`, named `identity`, acting for `principal`, committing
    /// at `epoch` or later: the version it reads at, the partition's visibility watermark.
    ///
    /// # Errors
    ///
    /// [`PartitionError::transaction_held`] for an identity the partition holds; the partition's refusal of the begin.
    pub fn bind(
        &mut self,
        key: BoundKey,
        identity: [u8; 16],
        (tenant, principal, epoch): (u64, u64, Epoch),
        now_us: u64,
    ) -> Result<u64, PartitionError> {
        if self.identities.contains_key(&identity) {
            return Err(PartitionError::transaction_held(identity));
        }
        let buffers = self.pipeline.buffers_mut().pop().unwrap_or_default();
        let begun = self
            .partition
            .begin(tenant, Mode::ReadWrite, 0, now_us, self.limits, buffers)?
            .with_principal(principal);
        let txn = if epoch == Epoch::ZERO {
            begun
        } else {
            begun.with_token(SessionToken::new(self.partition.id().get(), 0, epoch))
        };
        let read_version = txn.read_version();
        let _none = self.identities.insert(identity, key);
        let _previous = self.bound.insert(
            key,
            BoundTransaction {
                txn,
                identity,
                began_us: now_us,
                first: true,
                ticket: 0,
                sent_us: now_us,
                references: Vec::new(),
                operation: None,
                reading: false,
                unheld: BTreeSet::new(),
            },
        );
        Ok(read_version)
    }

    /// Takes the write `changes` of the transaction `key` at `now_us`: done at once when it references nothing to register,
    /// else waiting for the answers, while the caller holds what `changes` borrows.
    ///
    /// # Errors
    ///
    /// The transaction's refusal, which ends it, and its abort releases what it held.
    pub fn write_bound<W: Changes>(
        &mut self,
        key: BoundKey,
        changes: &W,
        now_us: u64,
    ) -> Result<BoundStep, PartitionError> {
        let open = self.open_epoch();
        let mut references = Vec::new();
        let partition = self.partition;
        let bound = self.held(key)?;
        let first = std::mem::replace(&mut bound.first, false);
        let read_epoch = Version::from_u64(bound.txn.read_version()).epoch();
        let routed = if first {
            partition.check_first_subject(changes, read_epoch)
        } else {
            Ok(())
        };
        let found = routed.and_then(|()| {
            partition.references(&mut bound.txn, changes, open, now_us, &mut references)
        });
        if let Err(refusal) = found {
            self.abort_bound(key);
            return Err(refusal);
        }
        if references.is_empty() {
            self.resume_write(key, changes, now_us)?;
            return Ok(BoundStep::Done);
        }
        self.ask(key, references, None, now_us)?;
        Ok(BoundStep::Waiting)
    }

    /// Runs the write `changes` of the transaction `key`, its references answered.
    ///
    /// # Errors
    ///
    /// The transaction's refusal, which ends it.
    pub fn resume_write<W: Changes>(
        &mut self,
        key: BoundKey,
        changes: &W,
        now_us: u64,
    ) -> Result<(), PartitionError> {
        let written = self.held(key)?.txn.write(changes, now_us);
        written.map_err(|refusal| {
            self.abort_bound(key);
            PartitionError::from(refusal)
        })
    }

    /// Takes the commit-time `operation` of the transaction `key`, at once or once its references are answered.
    ///
    /// # Errors
    ///
    /// The transaction's refusal, which ends it.
    pub fn operate_bound(
        &mut self,
        key: BoundKey,
        operation: Operation,
        now_us: u64,
    ) -> Result<BoundStep, PartitionError> {
        let open = self.open_epoch();
        let mut references = Vec::new();
        let partition = self.partition;
        let bound = self.held(key)?;
        let first = std::mem::replace(&mut bound.first, false);
        let tenant = bound.txn.tenant();
        let read_epoch = Version::from_u64(bound.txn.read_version()).epoch();
        let routed = if first {
            partition.check_operation_subject(tenant, &operation, read_epoch)
        } else {
            Ok(())
        };
        let found = routed.and_then(|()| {
            partition.operation_references(
                &mut bound.txn,
                tenant,
                &operation,
                open,
                now_us,
                &mut references,
            )
        });
        if let Err(refusal) = found {
            self.abort_bound(key);
            return Err(refusal);
        }
        if references.is_empty() {
            let operated = self.held(key)?.txn.operate(operation, now_us);
            return operated.map(|()| BoundStep::Done).map_err(|refusal| {
                self.abort_bound(key);
                PartitionError::from(refusal)
            });
        }
        self.ask(key, references, Some(operation), now_us)?;
        Ok(BoundStep::Waiting)
    }

    /// Commits the transaction `key` as `request` of `session`: the ticket its outcome comes under.
    ///
    /// # Errors
    ///
    /// The transaction's refusal; the pipeline's error, which stops it.
    pub fn commit_bound(
        &mut self,
        key: BoundKey,
        session: u128,
        request: u64,
        now_us: u64,
    ) -> Result<u64, PartitionError> {
        let bound = self
            .bound
            .remove(&key)
            .ok_or_else(|| PartitionError::from(TxnError::ended()))?;
        let _named = self.identities.remove(&bound.identity);
        let ticket = self.ticket();
        self.last_ticket = ticket;
        match bound.txn.commit(session, request, &[], now_us)? {
            Commit::ReadWrite(prepared) => self.pipeline.commit(ticket, prepared, now_us)?,
            Commit::ReadOnly(buffers) => self.pipeline.buffers_mut().push(buffers),
        }
        Ok(ticket)
    }

    /// Commits the one-frame write held as `key`, as request `request` of `session`: as [`PartitionHost::commit_bound`], and its
    /// resend reaches the session table as the first did (`PROTOCOL.md` §6.6).
    ///
    /// # Errors
    ///
    /// As [`PartitionHost::commit_bound`].
    pub fn commit_one_frame(
        &mut self,
        key: BoundKey,
        session: u128,
        request: u64,
        now_us: u64,
    ) -> Result<u64, PartitionError> {
        let bound = self
            .bound
            .remove(&key)
            .ok_or_else(|| PartitionError::from(TxnError::ended()))?;
        let _named = self.identities.remove(&bound.identity);
        let ticket = self.ticket();
        self.last_ticket = ticket;
        match bound.txn.commit(session, request, &[], now_us)? {
            Commit::ReadWrite(prepared) => {
                self.pipeline
                    .commit(ticket, prepared.with_one_frame(), now_us)?;
            }
            Commit::ReadOnly(buffers) => self.pipeline.buffers_mut().push(buffers),
        }
        Ok(ticket)
    }

    /// The session table's word on a one-frame write or a Drop of `session` numbered `request`: the stored reply of a resend, or
    /// `None` for a request to run.
    ///
    /// # Errors
    ///
    /// Code 14 for a request in the commit step, code 13 below the session's last, code 5 for a full table.
    pub fn resend_of(
        &self,
        session: u128,
        request: u64,
    ) -> Result<Option<StoredReply>, PartitionError> {
        self.pipeline
            .check_one_frame(session, request)
            .map_err(PartitionError::from)
    }

    /// Commits the Drop `drop` of `tenant` by `principal` as request `request` of `session`: the ticket its outcome comes under.
    ///
    /// # Errors
    ///
    /// [`PartitionError::cross_partition`] for a Drop reaching another partition; the store's refusal of it; the pipeline's
    /// error, which stops it.
    pub fn commit_drop(
        &mut self,
        (tenant, principal): (u64, u64),
        drop: DropRequest<'_>,
        (session, request): (u128, u64),
        now_us: u64,
    ) -> Result<u64, PartitionError> {
        let epoch = self.pipeline.delivered();
        self.partition.check_drop(Some(tenant), epoch)?;
        let buffers = self.pipeline.buffers_mut().pop().unwrap_or_default();
        let prepared = Prepared::drop(
            self.partition.applier(),
            tenant,
            drop,
            session,
            request,
            principal,
            buffers,
        )?;
        let ticket = self.ticket();
        self.last_ticket = ticket;
        self.pipeline.commit(ticket, prepared, now_us)?;
        Ok(ticket)
    }

    /// Hands the catalog version `contents` to the commit step: the ticket its outcome comes under, and the declarations the data
    /// refuses.
    ///
    /// # Errors
    ///
    /// The pipeline's refusal: a build running, which the version waits for, or a failure, which stops it.
    pub fn install_catalog(
        &mut self,
        contents: &CatalogContents,
        now_us: u64,
    ) -> Result<(u64, Vec<ChangesetError>), PartitionError> {
        let ticket = self.ticket();
        let refusals = self.pipeline.install(ticket, contents, now_us)?;
        Ok((ticket, refusals))
    }

    /// Aborts the transaction `key`, when it is open, and lets go of what it held: a dictionary entry it created is
    /// released at once, so the next writer of its term creates its own.
    pub fn abort_bound(&mut self, key: BoundKey) {
        if let Some(bound) = self.bound.remove(&key) {
            let _named = self.identities.remove(&bound.identity);
            let buffers = bound.txn.abort();
            self.pipeline.buffers_mut().push(buffers);
        }
    }

    /// Aborts every transaction the connection `connection` holds: what closed it never committed them.
    pub fn abort_connection(&mut self, connection: u64) {
        let held: Vec<BoundKey> = self
            .bound
            .keys()
            .filter(|(of, _)| *of == connection)
            .copied()
            .collect();
        for key in held {
            self.abort_bound(key);
        }
    }

    /// Aborts every transaction past the limit at `now_us`: those it aborted, which a client that vanished left open.
    pub fn sweep_bound(&mut self, now_us: u64) -> Vec<BoundKey> {
        let limit_us = u64::try_from(self.limits.read_write().as_micros()).unwrap_or(u64::MAX);
        let past: Vec<BoundKey> = self
            .bound
            .iter()
            .filter(|(_, bound)| now_us.saturating_sub(bound.began_us) > limit_us)
            .map(|(key, _)| *key)
            .collect();
        for key in &past {
            self.abort_bound(*key);
        }
        past
    }

    /// A close the partition asks for: the epoch its sequence would run out in.
    pub fn take_close_request(&mut self) -> Option<Epoch> {
        self.pipeline.take_close_request()
    }

    /// How long a read-write transaction lives, in milliseconds.
    pub fn bound_limit_ms(&self) -> u64 {
        u64::try_from(self.limits.read_write().as_millis()).unwrap_or(u64::MAX)
    }

    /// The bound transactions whose references were all answered since the last call.
    pub fn take_bound_ready(&mut self) -> Vec<BoundKey> {
        std::mem::take(&mut self.bound_ready)
    }

    /// Whether a read of the transaction `key` waits on a home's answer, behind which its Write, Operation or Commit waits its turn
    /// (`PROTOCOL.md` §13.13).
    pub fn read_waits(&self, key: BoundKey) -> bool {
        self.bound
            .get(&key)
            .is_some_and(|bound| bound.reading && !bound.references.is_empty())
    }

    /// The ticket the transaction `key` asks its current references under.
    pub fn bound_ticket(&self, key: BoundKey) -> Option<u64> {
        self.bound.get(&key).map(|bound| bound.ticket)
    }

    /// The bound transactions a home's refusal or a failed adoption ended since the last call, each with why. A request
    /// waiting on them gets that refusal.
    pub fn take_bound_refused(&mut self) -> Vec<(BoundKey, PartitionError)> {
        std::mem::take(&mut self.bound_refused)
    }

    /// The open epoch of the partition.
    pub(super) fn open_epoch(&self) -> Epoch {
        // Past the last epoch no close follows; the commit step refuses what it holds.
        self.pipeline.delivered().next().unwrap_or(Epoch::MAX)
    }

    pub(super) fn held(
        &mut self,
        key: BoundKey,
    ) -> Result<&mut BoundTransaction<'a, S, C, E, B>, PartitionError> {
        self.bound
            .get_mut(&key)
            .ok_or_else(|| PartitionError::from(TxnError::ended()))
    }

    /// Asks `references` for the transaction `key`, under a ticket of their own.
    pub(super) fn ask(
        &mut self,
        key: BoundKey,
        references: Vec<Reference>,
        operation: Option<Operation>,
        now_us: u64,
    ) -> Result<(), PartitionError> {
        let ticket = self.ticket();
        let bound = self.held(key)?;
        bound.ticket = ticket;
        bound.sent_us = now_us;
        bound.references = references.into_iter().map(|r| (r, None)).collect();
        bound.operation = operation;
        bound.reading = false;
        if let Some(asking) = self.bound.get(&key) {
            self.send_references(ticket, &asking.references);
        }
        Ok(())
    }

    /// Takes the answer to registration `index` of `ticket` when a bound transaction asked it: whether one did. A refusal
    /// ends the transaction, and [`PartitionHost::take_bound_refused`] names it, unless it says a read's skolem IRI is held
    /// by nobody, which that read answers with no entry.
    pub(super) fn bound_answered(
        &mut self,
        ticket: u64,
        index: usize,
        answer: &Result<Answer, PartitionError>,
        now_us: u64,
    ) -> bool {
        let found = self
            .bound
            .iter()
            .find(|(_, bound)| bound.ticket == ticket && !bound.references.is_empty())
            .map(|(key, _)| *key);
        let Some(key) = found else {
            return false;
        };
        match answer {
            Ok(answered) => {
                if let Some(bound) = self.bound.get_mut(&key)
                    && let Some((_, slot)) = bound.references.get_mut(index)
                {
                    *slot = Some(*answered);
                }
                self.finish_bound(key, now_us);
            }
            Err(refusal) if self.note_unheld(key, index, refusal) => {
                self.finish_bound(key, now_us);
            }
            Err(refusal) => {
                self.abort_bound(key);
                self.bound_refused.push((key, refusal.clone()));
            }
        }
        true
    }

    /// Whether `refusal` of registration `index` of `key` says the home holds no such skolem IRI, for a read: noted, so the read
    /// answers no entry and asks no more (`PROTOCOL.md` §13.13).
    fn note_unheld(&mut self, key: BoundKey, index: usize, refusal: &PartitionError) -> bool {
        let Some(bound) = self.bound.get_mut(&key) else {
            return false;
        };
        let tenant = bound.txn.tenant();
        let Some((reference, _)) = bound.references.get(index) else {
            return false;
        };
        let unheld =
            bound.reading && refusal.holds_no_skolem() && is_skolem(tenant, reference.iri());
        if unheld {
            let _new: bool = bound.unheld.insert(reference.iri().to_string());
        }
        unheld
    }

    /// Adopts the answers once every reference of `key` has one, and names it ready; a refusal of them ends it, and
    /// [`PartitionHost::take_bound_refused`] names it.
    fn finish_bound(&mut self, key: BoundKey, now_us: u64) {
        let partition = self.partition;
        let Some(bound) = self.bound.get_mut(&key) else {
            return;
        };
        let answered = |(reference, answer): &(Reference, Option<Answer>)| {
            answer.is_some() || bound.unheld.contains(reference.iri())
        };
        if !bound.references.iter().all(answered) {
            return;
        }
        let references = std::mem::take(&mut bound.references);
        bound.ticket = 0;
        let adopted = references.iter().try_for_each(|(reference, answer)| {
            answer.as_ref().map_or(Ok(()), |answer| {
                partition.adopt(&mut bound.txn, reference, answer, now_us)
            })
        });
        let operated = adopted.and_then(|()| match bound.operation.take() {
            Some(operation) => bound
                .txn
                .operate(operation, now_us)
                .map_err(PartitionError::from),
            None => Ok(()),
        });
        match operated {
            Ok(()) => self.bound_ready.push(key),
            Err(refusal) => {
                self.abort_bound(key);
                self.bound_refused.push((key, refusal));
            }
        }
    }

    /// Sends again the unanswered registrations of each bound transaction waiting past the resend interval.
    pub(super) fn resend_bound(&mut self, now_us: u64) {
        let due: Vec<BoundKey> = self
            .bound
            .iter()
            .filter(|(_, bound)| {
                !bound.references.is_empty() && now_us.saturating_sub(bound.sent_us) >= RESEND_US
            })
            .map(|(key, _)| *key)
            .collect();
        for key in due {
            if let Some(bound) = self.bound.get(&key) {
                self.send_references(bound.ticket, &bound.references);
                self.resends += 1;
            }
            if let Some(bound) = self.bound.get_mut(&key) {
                bound.sent_us = now_us;
            }
        }
    }
}
