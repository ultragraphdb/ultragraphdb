/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/lists.go (viLocalCache's methods). Changed for ultragraphdb: translated to
 * Rust; a search reads through a request as an insert does.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_index::LocalCache;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::hnsw_error::HnswError;
use crate::types::vi_txn::ViTxn;

impl<S, C> LocalCache for ViTxn<'_, '_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    type Error = HnswError;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.value(key)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.value(key)
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.find_value(prefix, filter)
    }
}
