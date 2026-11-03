/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/index.go (Txn). Changed for ultragraphdb: translated to Rust; Go's
 * `error` as an associated type, and Go's `context.Context` left to the implementation.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

use crate::types::key_value::KeyValue;

/// A transaction of a persistent store, in which a vector index reads and writes.
pub trait Txn {
    /// Why an operation failed.
    type Error;

    /// The transaction's start timestamp.
    fn start_ts(&self) -> u64;

    /// The value under `key`.
    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error>;

    /// The value under `key`, with the key's lock held.
    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error>;

    /// The ID of the first entry under `prefix`, in key order, whose value passes `filter`; `None` when none passes,
    /// where Dgraph returns the uid 0.
    fn find(
        &self,
        prefix: &[u8],
        filter: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<SystemId>, Self::Error>;

    /// Adds a mutation of `key`.
    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error>;

    /// Adds a mutation of `key`, with the key's lock held.
    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error>;

    /// Locks `key`.
    fn lock_key(&self, key: &[u8]);

    /// Unlocks `key`.
    fn unlock_key(&self, key: &[u8]);
}
