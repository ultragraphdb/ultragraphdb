/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use oxrdf::{GraphName, NamedNode, NamedNodeRef};
use ultragraphdb_core_ids::{Epoch, SystemId};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{CachedList, ReadPin};
use ultragraphdb_store_quads::{KernelScan, Page, QuadError};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_log::OrderedLog;
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_commit::{CommitError, StoredReply};

use crate::errors::partition_error::PartitionError;
use crate::types::bound_read::BoundRead;
use crate::types::bound_transaction::{BoundKey, BoundStep};
use crate::types::partition_host::PartitionHost;

/// What the partition's read kernel answers beside its writes (`PROTOCOL.md` §§13.11, 13.13, 13.17): a bound transaction's
/// reads, recorded in its reads and ending it when they fail on it; plain reads of the partition at a version; and what a
/// read is told of the partition's closes. The frames they fill are the caller's.
impl<'a, L, S, C, E, B> PartitionHost<'a, L, S, C, E, B>
where
    L: OrderedLog,
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The transaction a Bind named `identity`, while the partition holds it.
    pub fn bound_read(&self, identity: &[u8; 16]) -> Option<BoundRead> {
        let key = *self.identities.get(identity)?;
        let bound = self.bound.get(&key)?;
        Some(BoundRead::new(
            key,
            bound.txn.read_version(),
            bound.txn.principal(),
            bound.txn.tenant(),
        ))
    }

    /// Asks the homes of the IRIs of `iris` the transaction `key` reads and neither the partition nor the transaction holds an
    /// ID for, as a read of them does ([`crate::Partition::read_references`]): done at once when none is left to ask, else
    /// waiting for the answers, which [`PartitionHost::take_bound_ready`] then names. While the transaction already waits on
    /// answers this asks nothing and waits too, to be asked again once they came. An IRI a home holds none of, a skolem IRI, is
    /// left for the dictionary to answer with no entry.
    ///
    /// # Errors
    ///
    /// The transaction's refusal or the store's error, which ends it.
    pub fn read_terms_bound(
        &mut self,
        key: BoundKey,
        iris: &[NamedNode],
        now_us: u64,
    ) -> Result<BoundStep, PartitionError> {
        let open = self.open_epoch();
        let partition = self.partition;
        let bound = self.held(key)?;
        if !bound.references.is_empty() {
            return Ok(BoundStep::Waiting);
        }
        let tenant = bound.txn.tenant();
        let mut references = Vec::new();
        let found = iris
            .iter()
            .filter(|iri| {
                !bound.txn.knows_iri(iri.as_str()) && !bound.unheld.contains(iri.as_str())
            })
            .try_for_each(|iri| {
                partition.read_references(
                    tenant,
                    &GraphName::DefaultGraph,
                    iri,
                    open,
                    &mut references,
                )
            });
        if let Err(refusal) = found {
            self.abort_bound(key);
            return Err(refusal);
        }
        if references.is_empty() {
            return Ok(BoundStep::Done);
        }
        self.ask(key, references, None, now_us)?;
        self.held(key)?.reading = true;
        Ok(BoundStep::Waiting)
    }

    /// A page of the lists `scan` names as the transaction `key` sees them at `now_us` ([`ultragraphdb_transactions_txn::Transaction::page`]).
    ///
    /// # Errors
    ///
    /// The transaction's refusal, which ends it, and its abort releases what it held; what `sink` took is not to be sent.
    pub fn page_bound<F>(
        &mut self,
        key: BoundKey,
        scan: &KernelScan,
        now_us: u64,
        sink: F,
    ) -> Result<Page, PartitionError>
    where
        F: FnMut(&[u8], &EncodedTerm, &[SystemId]) -> ControlFlow<()>,
    {
        let paged = self.held(key)?.txn.page(scan, now_us, sink);
        paged.map_err(|refusal| {
            self.abort_bound(key);
            refusal.into()
        })
    }

    /// The ID the transaction `key` reads `iri` by at `now_us` ([`ultragraphdb_transactions_txn::Transaction::resolve_iri`]).
    ///
    /// # Errors
    ///
    /// The transaction's refusal, which ends it.
    pub fn resolve_bound(
        &mut self,
        key: BoundKey,
        iri: NamedNodeRef<'_>,
        now_us: u64,
    ) -> Result<Option<SystemId>, PartitionError> {
        let resolved = self.held(key)?.txn.resolve_iri(iri, now_us);
        resolved.map_err(|refusal| {
            self.abort_bound(key);
            refusal.into()
        })
    }

    /// The value of the dictionary's `entry` as the transaction `key` sees it at `now_us`: a term it created is found.
    ///
    /// # Errors
    ///
    /// The transaction's refusal, which ends it.
    pub fn dictionary_bound(
        &mut self,
        key: BoundKey,
        entry: &[u8],
        now_us: u64,
    ) -> Result<Option<Vec<u8>>, PartitionError> {
        let mut out = Vec::with_capacity(1);
        let read = self
            .held(key)?
            .txn
            .dictionary_into(&[entry], now_us, &mut out);
        read.map_err(|refusal| {
            self.abort_bound(key);
            PartitionError::from(refusal)
        })?;
        Ok(out.pop().flatten())
    }

    /// Keeps `version` readable until the pin is dropped, however many turns the read takes.
    ///
    /// # Errors
    ///
    /// The posting lists' refusal of a version below what the store has discarded.
    pub fn pin(&self, version: u64) -> Result<ReadPin<'a, S, C>, PartitionError> {
        let lists = self.partition.applier().quads().lists();
        lists
            .pin(version)
            .map_err(|refused| QuadError::from(refused).into())
    }

    /// A page of the lists `scan` names at its version ([`ultragraphdb_store_quads::QuadStore::page`]).
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn page<F>(&self, scan: &KernelScan, sink: F) -> Result<Page, PartitionError>
    where
        F: FnMut(&[u8], &EncodedTerm, &[SystemId]) -> ControlFlow<()>,
    {
        Ok(self.partition.applier().quads().page(scan, sink)?)
    }

    /// A page of the referrer sets `scan` names at its version ([`ultragraphdb_store_quads::QuadStore::values_page`]).
    ///
    /// # Errors
    ///
    /// The store's error.
    pub fn referrers_page<F>(&self, scan: &KernelScan, sink: F) -> Result<Page, PartitionError>
    where
        F: FnMut(&[u8], &[u8]) -> ControlFlow<()>,
    {
        Ok(self.partition.applier().quads().values_page(scan, sink)?)
    }

    /// The value of the dictionary's `entry`, as the store holds it.
    ///
    /// # Errors
    ///
    /// The store's error.
    pub fn dictionary(&self, entry: &[u8]) -> Result<Option<Vec<u8>>, PartitionError> {
        let quads = self.partition.applier().quads();
        quads
            .store()
            .get_versionless(entry)
            .map_err(|failed| QuadError::from(failed).into())
    }

    /// The newest epoch the partition has closed: its close delivered and every round stamped at or below it applied.
    pub fn closed(&self) -> Epoch {
        self.pipeline.step().closed()
    }

    /// The partition's visibility watermark.
    pub fn watermark(&self) -> u64 {
        self.pipeline.watermark()
    }

    /// The reply the session table holds for `request` of `session` ([`ultragraphdb_transactions_commit::CommitStep::retry`]).
    ///
    /// # Errors
    ///
    /// Why it holds none: [`CommitError::session_busy`], [`CommitError::request_out_of_order`], [`CommitError::not_committed`].
    pub fn recall(&self, session: u128, request: u64) -> Result<StoredReply, CommitError> {
        self.pipeline.retry(session, request)
    }
}
