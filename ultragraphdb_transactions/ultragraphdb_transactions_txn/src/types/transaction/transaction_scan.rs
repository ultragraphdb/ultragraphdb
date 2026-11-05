/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use oxrdf::NamedNodeRef;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{KernelRow, KernelScan, Page};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::txn_error::TxnError;
use crate::types::read_set::{SPAN_BYTES, TERM_KEY};
use crate::types::transaction::Transaction;

/// What a Scan naming a transaction reads (`PROTOCOL.md` §13.13): the store at the transaction's version with its uncommitted
/// writes over it, what makes its commit serializable recorded before anything leaves.
impl<S, C, E, B> Transaction<'_, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The lists `scan` names, as the transaction sees them, in key order, at most its limit of them, appended to `out`, at
    /// `now_us`; read at the transaction's own version. Nothing is held. The span it covered is in the read set before it
    /// returns: to the range's past key when the range ended, found nothing included, and to just past the last row when stopped
    /// at the limit, the key a next scan begins at, which extends the same range. A span of the keyspace of lists is a range
    /// scanned, so every list a commit writes in it conflicts.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::read_set_too_large`] for a span the read set has
    /// no room for, ending it, with nothing added to `out`; [`TxnError::changeset`] for a store that failed or a keyspace
    /// holding no lists, ending it.
    pub fn scan_into(
        &mut self,
        scan: &KernelScan,
        now_us: u64,
        out: &mut Vec<KernelRow>,
    ) -> Result<Option<Vec<u8>>, TxnError> {
        let before = out.len();
        match self.page(scan, now_us, KernelRow::gather(out)) {
            Ok(page) => Ok(page.into_end().resume_from()),
            Err(e) => {
                out.truncate(before);
                Err(e)
            }
        }
    }

    /// A page of the lists `scan` names, as the transaction sees them, in key order, streamed to `sink` object by object at
    /// `now_us`, read at the transaction's own version and holding nothing. The span it covered is in the read set before it
    /// returns, extending the span it resumes from, and what `sink` took may leave only once it returns: to the range's past key
    /// when the range ended, found nothing included, to just past the last key `sink` took anything of when stopped at the
    /// limit or paused, and nothing when `sink` took nothing.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::read_set_too_large`] for a span the read set has
    /// no room for, ending it, so what `sink` took is not to be sent; [`TxnError::changeset`] for a store that failed or a
    /// keyspace holding no lists, ending it.
    pub fn page<F>(&mut self, scan: &KernelScan, now_us: u64, sink: F) -> Result<Page, TxnError>
    where
        F: FnMut(&[u8], &EncodedTerm, &[SystemId]) -> ControlFlow<()>,
    {
        let limit = self.limits.max_read_set();
        let execution = self.live(now_us)?;
        let page = match execution.page(scan, sink) {
            Ok(page) => page,
            Err(e) => {
                self.end();
                return Err(e.into());
            }
        };
        self.span_fits(limit)?;
        Ok(page)
    }

    /// A page of the stored values `scan` names, as the transaction sees them: the statements' stored bytes at its version with
    /// the records it wrote over them, streamed to `sink` at `now_us` and holding nothing. Its span is a range scanned, in the
    /// read set before it returns as [`Transaction::page`] records one, and what `sink` took may leave only once it returns.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::read_set_too_large`] for a span the read set has
    /// no room for, ending it, so what `sink` took is not to be sent; [`TxnError::changeset`] for a store that failed or a
    /// keyspace holding no stored values, ending it.
    pub fn values_page<F>(
        &mut self,
        scan: &KernelScan,
        now_us: u64,
        sink: F,
    ) -> Result<Page, TxnError>
    where
        F: FnMut(&[u8], &[u8]) -> ControlFlow<()>,
    {
        let limit = self.limits.max_read_set();
        let execution = self.live(now_us)?;
        let page = match execution.values_page(scan, sink) {
            Ok(page) => page,
            Err(e) => {
                self.end();
                return Err(e.into());
            }
        };
        self.span_fits(limit)?;
        Ok(page)
    }

    /// Refuses a read set grown past `limit` by the span just recorded, ending the transaction.
    fn span_fits(&mut self, limit: usize) -> Result<(), TxnError> {
        let Some(execution) = self.execution.as_mut() else {
            return Ok(());
        };
        let counts = execution.read_counts();
        let size = counts.key_bytes()
            + TERM_KEY * counts.absent()
            + SPAN_BYTES
                * (counts.lists() + counts.absent() + 2 * (counts.statements() + counts.ranges()));
        if size > limit {
            self.end();
            return Err(TxnError::read_set_too_large(size, limit));
        }
        Ok(())
    }

    /// The ID the transaction reads `iri` by at `now_us`, by the path its reads take: got or created, the same ID any concurrent
    /// writer of the IRI takes, one it created recorded as an absent term, so a later commit of a statement of it conflicts. An
    /// IRI another partition owns answers the partition's copy of it, or the ID its home gave ([`Transaction::adopt`]). `None`,
    /// recording nothing, for an unheld skolem IRI, which no read can hold.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::changeset`] for a refusal of the ID, an IRI another
    /// partition owns with neither a copy nor an adopted ID among them, or a store that failed, ending it.
    pub fn resolve_iri(
        &mut self,
        iri: NamedNodeRef<'_>,
        now_us: u64,
    ) -> Result<Option<SystemId>, TxnError> {
        let execution = self.live(now_us)?;
        match execution.find_iri(iri) {
            Ok(id) => Ok(id),
            Err(e) => {
                self.end();
                Err(e.into())
            }
        }
    }

    /// Whether the transaction resolved `iri` already, so a read of it asks its home for nothing; false for an ended one.
    pub fn knows_iri(&mut self, iri: &str) -> bool {
        self.execution.as_mut().is_some_and(|e| e.knows_iri(iri))
    }

    /// The value of each of `keys` in the dictionary as the transaction sees it at `now_us`, into `out`, cleared first: a term it
    /// created is found. A range of the dictionary records no range.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::changeset`] for a store that failed, ending it.
    pub fn dictionary_into(
        &mut self,
        keys: &[&[u8]],
        now_us: u64,
        out: &mut Vec<Option<Vec<u8>>>,
    ) -> Result<(), TxnError> {
        let execution = self.live(now_us)?;
        if let Err(e) = execution.dictionary_into(keys, out) {
            self.end();
            return Err(e.into());
        }
        Ok(())
    }

    /// The key ranges the transaction scanned so far, as [`ultragraphdb_store_apply::Execution::scanned_ranges`] gives them;
    /// none for an ended one.
    pub fn scanned_ranges(&mut self) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        self.execution
            .as_mut()
            .map_or_else(Vec::new, |e| e.scanned_ranges())
    }

    /// How many lists the transaction holds in memory; none for an ended one.
    pub fn held_lists(&mut self) -> usize {
        self.execution.as_mut().map_or(0, |e| e.held_lists())
    }
}
