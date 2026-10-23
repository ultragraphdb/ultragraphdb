/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (TxnCache's Get, Ts and Find). Changed for ultragraphdb: translated
 * to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::{CacheType, Txn};

use crate::types::txn_cache::TxnCache;

impl<X: Txn> CacheType for TxnCache<'_, X> {
    type Error = X::Error;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.txn.get(key)
    }

    fn ts(&self) -> u64 {
        self.start_ts
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.txn.find(prefix, filter)
    }
}
