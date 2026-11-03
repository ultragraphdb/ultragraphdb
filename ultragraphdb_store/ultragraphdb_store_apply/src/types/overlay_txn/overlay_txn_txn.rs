/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_store_hnsw::HnswError;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_index::{KeyValue, Txn};
use ultragraphdb_traits_store::{ListCache, Store};

use crate::types::overlay_txn::OverlayTxn;

impl<S, C, B> Txn for OverlayTxn<'_, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    type Error = HnswError;

    fn start_ts(&self) -> u64 {
        self.read_ts
    }

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.value(key)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.value(key)
    }

    fn find(
        &self,
        prefix: &[u8],
        filter: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<SystemId>, Self::Error> {
        self.first_passing(prefix, filter)
    }

    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.write(key, t.value());
        Ok(())
    }

    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.write(key, t.value());
        Ok(())
    }

    fn lock_key(&self, _: &[u8]) {}

    fn unlock_key(&self, _: &[u8]) {}
}
