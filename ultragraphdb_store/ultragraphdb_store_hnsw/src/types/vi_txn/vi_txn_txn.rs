/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/oracle.go (viTxn's methods). Changed for ultragraphdb: translated to Rust; a
 * request owns its lists, so locking and unlocking a key do nothing and the variants with the lock held are the
 * same reads and writes.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_index::{KeyValue, Txn};
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::hnsw_error::HnswError;
use crate::types::vi_txn::ViTxn;

impl<S, C> Txn for ViTxn<'_, '_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    type Error = HnswError;

    fn start_ts(&self) -> u64 {
        self.read_ts()
    }

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.value(key)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.value(key)
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.find_value(prefix, filter)
    }

    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.write(key, t)
    }

    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.write(key, t)
    }

    fn lock_key(&self, _: &[u8]) {}

    fn unlock_key(&self, _: &[u8]) {}
}
