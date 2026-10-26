/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::sync::Mutex;

use ultragraphdb_traits_store::{PersistMode, Store, WriteBatch};

use crate::errors::quad_error::QuadError;
use crate::utils::dictionary_keys::{STATEMENT_LEASE, UID_LEASE, lease_key};
use crate::utils::lock::lock;

/// How many numbers one lease covers.
pub const LEASE_BLOCK: u64 = 10_000;
/// The highest UID: the top 16 bits are the entity-type prefix, zero in this stage.
pub const MAX_UID: u64 = (1 << 48) - 1;

/// The numbers a namespace hands out from leased blocks, UIDs and statement IDs each a sequence of its own from 1:
/// the high-water mark of a block is written to the keyspace without versions and synced before any number of the
/// block is handed out, so a crash never reissues one, as Dgraph's Zero leases UIDs. A number is never reused; one a
/// refused write drew is spent.
#[derive(Debug, Default)]
pub struct Leases {
    /// By namespace and sequence: the next number and the end of the lease, exclusive.
    held: Mutex<BTreeMap<(u64, u8), (u64, u64)>>,
}

impl Leases {
    pub fn new() -> Self {
        Self::default()
    }

    /// The next UID of `namespace`, never 0 and never above [`MAX_UID`].
    pub fn next_uid<S: Store>(&self, store: &S, namespace: u64) -> Result<u64, QuadError> {
        let uid = self.next(store, namespace, UID_LEASE)?;
        if uid > MAX_UID {
            return Err(QuadError::UidsExhausted(namespace));
        }
        Ok(uid)
    }

    /// The next statement ID of `namespace`, never 0.
    pub fn next_statement_id<S: Store>(&self, store: &S, namespace: u64) -> Result<u64, QuadError> {
        self.next(store, namespace, STATEMENT_LEASE)
    }

    fn next<S: Store>(&self, store: &S, namespace: u64, tag: u8) -> Result<u64, QuadError> {
        let mut held = lock(&self.held);
        let (next, end) = match held.get(&(namespace, tag)) {
            Some(&(next, end)) if next < end => (next, end),
            Some(&(_, end)) => self.lease(store, namespace, tag, end)?,
            None => {
                let mark = store
                    .get_versionless(&lease_key(namespace, tag))?
                    .map(|v| decode_mark(&lease_key(namespace, tag), &v))
                    .transpose()?
                    .unwrap_or(1);
                self.lease(store, namespace, tag, mark)?
            }
        };
        held.insert((namespace, tag), (next + 1, end));
        Ok(next)
    }

    /// Leases the block from `from` on: its end synced before any of it is handed out.
    fn lease<S: Store>(
        &self,
        store: &S,
        namespace: u64,
        tag: u8,
        from: u64,
    ) -> Result<(u64, u64), QuadError> {
        let end = from + LEASE_BLOCK;
        let mut batch = WriteBatch::new();
        batch.put_versionless(lease_key(namespace, tag), end.to_be_bytes().to_vec());
        store.apply(batch, PersistMode::Sync)?;
        Ok((from, end))
    }
}

fn decode_mark(key: &[u8], value: &[u8]) -> Result<u64, QuadError> {
    let raw: [u8; 8] = value
        .try_into()
        .map_err(|_| QuadError::Corrupted(key, "a lease mark is not 8 bytes"))?;
    Ok(u64::from_be_bytes(raw))
}
