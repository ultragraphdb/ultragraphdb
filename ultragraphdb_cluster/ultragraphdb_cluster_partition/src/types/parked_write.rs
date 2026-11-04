/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_transactions_txn::Transaction;

use crate::types::answer::Answer;
use crate::types::reference::Reference;

/// A write waiting for its registrations' answers: its transaction, begun at the partition, what it writes, and each
/// reference with its answer so far.
#[derive(Debug)]
pub(crate) struct ParkedWrite<'a, S, C, E, B>
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
    pub changeset: Changeset,
    pub references: Vec<(Reference, Option<Answer>)>,
    pub began_us: u64,
    pub sent_us: u64,
}
