/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (rebuilder.Run: the stream's KeyToListWithThreadId and
 * FinishThread, which write a thread's deltas to the temporary Badger once it holds 10,000 lists, and the second
 * stream's rollup of each key at MaxUint64). Changed for ultragraphdb: translated to Rust; the temporary store is a
 * temporary keyspace of the store, holding the build's deltas in one region and the swap's final entries in
 * another; the bound counts postings and buffered entries rather than lists, so a list every subject shares is
 * written out as well.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_proto_pb::pb::Posting;
use ultragraphdb_store_posting::{BIT_DELTA_POSTING, List, marshal};
use ultragraphdb_traits_store::{Entry, Item, Store, Temporary, WriteBatch};

/// The postings and entries a build holds in memory before it writes them to its temporary keyspace.
pub(crate) const HELD_BOUND: usize = 10_000;
/// The region of the temporary keyspace holding the build's deltas, by index key.
pub(crate) const DELTAS: u8 = 0x00;
/// The region of the temporary keyspace holding the swap's entries, final.
pub(crate) const SWAP: u8 = 0x01;
/// The meta byte of a deletion among the swap's entries: a read of the temporary keyspace reports no deletion, so
/// a deletion is kept there as a value of this meta byte, which no posting list carries.
const DELETION: u8 = 0x00;

/// What a build holds in memory: the index lists it adds postings to until they reach [`HELD_BOUND`], when they
/// are written to the temporary keyspace as deltas at a version of their own, and the swap's entries until as many
/// are buffered. It counts what it held at most and the postings it added.
#[derive(Debug)]
pub(crate) struct BuildBuffer<'t> {
    temporary: &'t Temporary,
    start_ts: u64,
    lists: BTreeMap<Vec<u8>, List>,
    postings: usize,
    swap: WriteBatch,
    peak: usize,
    added: u64,
    version: u64,
}

/// `key` in `region` of the temporary keyspace.
pub(crate) fn in_region(region: u8, key: &[u8]) -> Vec<u8> {
    let mut k = Vec::with_capacity(key.len() + 1);
    k.push(region);
    k.extend_from_slice(key);
    k
}

/// The swap's entry an item of the swap region holds, its region byte taken off.
pub(crate) fn swap_entry(item: Item) -> Entry {
    let key = item.key()[1..].to_vec();
    let (version, meta, discard) = (item.version(), item.meta(), item.discard());
    if meta == DELETION {
        return Entry::deletion(key, version);
    }
    let entry = Entry::put(key, version, meta, item.into_value());
    if discard { entry.with_discard() } else { entry }
}

/// The item a delta region's item holds, its region byte taken off.
pub(crate) fn delta_item(item: Item) -> Item {
    let key = item.key()[1..].to_vec();
    Item::new(
        key,
        item.version(),
        item.meta(),
        item.discard(),
        item.into_value(),
    )
}

impl<'t> BuildBuffer<'t> {
    pub fn new(temporary: &'t Temporary, start_ts: u64) -> Self {
        Self {
            temporary,
            start_ts,
            lists: BTreeMap::new(),
            postings: 0,
            swap: WriteBatch::new(),
            peak: 0,
            added: 0,
            version: 1,
        }
    }

    /// The most postings and entries held at once.
    pub fn peak(&self) -> usize {
        self.peak
    }

    /// The postings added to index lists.
    pub fn added(&self) -> u64 {
        self.added
    }

    fn held(&self) -> usize {
        self.postings + self.swap.len()
    }

    /// Adds `posting` to the list of `key`, as a quad list holds it: a uid, or a literal's value posting.
    pub fn add_posting(&mut self, key: Vec<u8>, posting: Posting) {
        let start_ts = self.start_ts;
        let list = self.lists.entry(key.clone()).or_insert_with(|| {
            let mut l = List::empty(key);
            l.set_ts(start_ts);
            l
        });
        list.add_posting(posting, false);
        self.postings += 1;
        self.added += 1;
        self.peak = self.peak.max(self.held());
    }

    /// Buffers `entries` for the swap.
    pub fn put_swap(&mut self, entries: Vec<Entry>) {
        for e in entries {
            let key = in_region(SWAP, e.key());
            let held = match e.value() {
                None => Entry::put(key, e.version(), DELETION, Vec::new()),
                Some(value) if e.discard() => {
                    Entry::put(key, e.version(), e.meta(), value.to_vec()).with_discard()
                }
                Some(value) => Entry::put(key, e.version(), e.meta(), value.to_vec()),
            };
            self.swap.push(held);
        }
        self.peak = self.peak.max(self.held());
    }

    /// Writes what is held to the temporary keyspace once it reaches the bound.
    pub fn maybe_flush<S: Store>(&mut self, store: &S) -> Result<(), StoreError> {
        if self.held() >= HELD_BOUND {
            self.flush(store)?;
        }
        Ok(())
    }

    /// Writes every list held as a delta at the next version of the delta region, and every swap entry buffered,
    /// to the temporary keyspace.
    pub fn flush<S: Store>(&mut self, store: &S) -> Result<(), StoreError> {
        let mut batch = std::mem::take(&mut self.swap);
        for (key, list) in std::mem::take(&mut self.lists) {
            if let Some(delta) = list.delta() {
                batch.push(Entry::put(
                    in_region(DELTAS, &key),
                    self.version,
                    BIT_DELTA_POSTING,
                    marshal(delta),
                ));
            }
        }
        self.postings = 0;
        self.version += 1;
        if !batch.is_empty() {
            store.apply_temporary(self.temporary, batch)?;
        }
        Ok(())
    }
}
