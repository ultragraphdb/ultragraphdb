/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sim_snapshot_drop;
mod sim_snapshot_store_snapshot;

use std::sync::{Arc, Mutex};

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Item, Scan, key_namespace};

use crate::types::sim_store::Inner;
use crate::types::sim_store::state::{State, decode};

/// A snapshot of a [`crate::SimStore`]: the state when it was opened, read at one timestamp. While it is open, the
/// store's discard timestamp stays at or below its read timestamp. A snapshot of a temporary keyspace reads every
/// version it holds, holds no discard timestamp back, and draws read failures but no corruption.
#[derive(Debug)]
pub struct SimSnapshot {
    shared: Arc<Mutex<Inner>>,
    state: Arc<State>,
    read_ts: u64,
    id: u64,
    generation: u64,
    temporary: bool,
}

impl SimSnapshot {
    pub(crate) fn new(
        shared: Arc<Mutex<Inner>>,
        state: Arc<State>,
        read_ts: u64,
        id: u64,
        generation: u64,
    ) -> Self {
        Self {
            shared,
            state,
            read_ts,
            id,
            generation,
            temporary: false,
        }
    }

    /// The snapshot of a temporary keyspace holding `state`.
    pub(crate) fn temporary(shared: Arc<Mutex<Inner>>, state: Arc<State>, id: u64) -> Self {
        Self {
            shared,
            state,
            read_ts: u64::MAX,
            id,
            generation: 0,
            temporary: true,
        }
    }

    /// The number the store's log names this snapshot by.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The bytes of `key` at `version` as a read finds them: corrupted as the store's data is, unless the snapshot
    /// reads a temporary keyspace.
    fn bytes(&self, inner: &Inner, key: &[u8], version: u64, stored: &[u8]) -> Vec<u8> {
        if self.temporary {
            stored.to_vec()
        } else {
            inner.corrupted.bytes(key, version, stored)
        }
    }

    fn banned(&self, key: &[u8]) -> bool {
        key_namespace(key).is_some_and(|ns| self.state.banned.contains(&ns))
    }

    /// The newest version of `key` at or below the read timestamp, as `inner`'s corruptions leave its bytes.
    fn read(&self, inner: &Inner, key: &[u8]) -> Result<Option<Item>, StoreError> {
        if self.banned(key) {
            return Ok(None);
        }
        let Some((version, stored)) = self.state.versions(key).find(|(v, _)| *v <= self.read_ts)
        else {
            return Ok(None);
        };
        let decoded = decode(&self.bytes(inner, key, version, stored))
            .map_err(|reason| StoreError::Corrupted(key.to_vec(), reason))?;
        Ok((!decoded.deletion).then(|| {
            Item::new(
                key.to_vec(),
                version,
                decoded.meta,
                decoded.discard,
                decoded.value,
            )
        }))
    }

    /// The items `scan` reports, an undecodable version ending them with its error.
    fn scan_items(&self, inner: &Inner, scan: &Scan) -> Vec<Result<Item, StoreError>> {
        let mut keys: Vec<Vec<Result<Item, StoreError>>> = Vec::new();
        let mut current: Option<&[u8]> = None;
        let mut done = false;
        for ((key, std::cmp::Reverse(version)), stored) in self.state.range(scan) {
            if current != Some(key.as_slice()) {
                current = Some(key.as_slice());
                done = !scan.selects(key) || self.banned(key);
                if !done {
                    keys.push(Vec::new());
                }
            }
            if done || *version > self.read_ts {
                continue;
            }
            let Some(items) = keys.last_mut() else {
                continue;
            };
            match decode(&self.bytes(inner, key, *version, stored)) {
                Err(reason) => {
                    items.push(Err(StoreError::Corrupted(key.clone(), reason)));
                    done = true;
                }
                Ok(d) if d.deletion => done = true,
                Ok(d) => {
                    items.push(Ok(Item::new(
                        key.clone(),
                        *version,
                        d.meta,
                        d.discard,
                        d.value,
                    )));
                    done = !scan.is_all_versions();
                }
            }
        }
        if scan.direction() == ultragraphdb_traits_store::Direction::Reverse {
            keys.reverse();
        }
        let mut items: Vec<Result<Item, StoreError>> = keys.into_iter().flatten().collect();
        if let Some(at) = items.iter().position(Result::is_err) {
            items.truncate(at + 1);
        }
        items
    }
}
