/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (TxnCache, NewTxnCache). Changed for ultragraphdb: translated to
 * Rust; the transaction is borrowed.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod txn_cache_cache_type;

use ultragraphdb_traits_index::Txn;

/// Dgraph's `TxnCache`: the cache an insert reads through, over the transaction it writes in.
#[derive(Debug)]
pub struct TxnCache<'t, X> {
    txn: &'t X,
    start_ts: u64,
}

impl<'t, X: Txn> TxnCache<'t, X> {
    /// Dgraph's `NewTxnCache`.
    pub fn new(txn: &'t X, start_ts: u64) -> Self {
        Self { txn, start_ts }
    }

    /// The transaction an insert writes in.
    pub fn txn(&self) -> &'t X {
        self.txn
    }
}
