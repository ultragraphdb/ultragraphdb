/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_store_apply::Operation;
use ultragraphdb_transactions_txn::Transaction;

use crate::types::answer::Answer;
use crate::types::reference::Reference;

/// A bound transaction's name at its host: the connection and the stream its Bind opened.
pub type BoundKey = (u64, u64);

/// What a bound transaction's request came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundStep {
    /// Done now.
    Done,
    /// Waiting for its references' answers: [`crate::PartitionHost::take_bound_ready`] names the transaction once they
    /// came, and a write then runs through [`crate::PartitionHost::resume_write`].
    Waiting,
}

/// A transaction a Bind opened at the partition (`PROTOCOL.md` §13.10), held between its requests.
#[derive(Debug)]
pub(crate) struct BoundTransaction<'a, S, C, E, B>
where
    S: ultragraphdb_traits_store::Store + Send + Sync + 'static,
    C: ultragraphdb_traits_store::ListCache<Value = ultragraphdb_store_posting::CachedList>
        + Send
        + Sync
        + 'static,
    B: ultragraphdb_traits_blocks::BlockSource + Send + Sync + 'static,
    E: ultragraphdb_traits_store::Executor,
{
    pub txn: Transaction<'a, S, C, E, B>,
    /// The identity its Bind named, which Scans of it name.
    pub identity: [u8; 16],
    pub began_us: u64,
    /// Whether no request of it has reached the partition yet: a first request's first subject outside the partition is the
    /// sender's stale map, code 26, and any later one is cross-partition.
    pub first: bool,
    /// The ticket its current references are asked under, and when they went last.
    pub ticket: u64,
    pub sent_us: u64,
    pub references: Vec<(Reference, Option<Answer>)>,
    /// A commit-time operation waiting for its references, applied when they come.
    pub operation: Option<Operation>,
    /// Whether its current references are a read's, which a home holding no such skolem IRI leaves open.
    pub reading: bool,
    /// The skolem IRIs their homes hold none of, which its reads answer with no entry without asking again.
    pub unheld: BTreeSet<String>,
}
