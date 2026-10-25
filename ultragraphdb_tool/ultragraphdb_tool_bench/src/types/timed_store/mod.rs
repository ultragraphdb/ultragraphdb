/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod timed_store_store;

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ultragraphdb_store_kv::FjallStore;

use crate::types::write_call::WriteCall;

/// The fjall store with every write call timed: each commit batch's call with how long `Store::apply` took, its
/// entries and the data keyspace's level-0 tables and sealed memtables after it; the other calls, rollup batches,
/// counted with their total time. fjall 3.1.12 reports no write stall,
/// so the time in the write call and the pressure that would stall it are what the run reads.
#[derive(Debug)]
pub struct TimedStore {
    inner: FjallStore,
    calls: Mutex<Vec<WriteCall>>,
    other_calls: AtomicU64,
    other_nanos: AtomicU64,
}

impl TimedStore {
    pub fn new(inner: FjallStore) -> Self {
        Self {
            inner,
            calls: Mutex::new(Vec::new()),
            other_calls: AtomicU64::new(0),
            other_nanos: AtomicU64::new(0),
        }
    }

    pub fn inner(&self) -> &FjallStore {
        &self.inner
    }

    /// The commit batches' write calls recorded since the last take.
    pub fn take_calls(&self) -> Vec<WriteCall> {
        std::mem::take(
            &mut *self
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// The other write calls so far, and their total time in nanoseconds.
    pub fn other_calls(&self) -> (u64, u64) {
        (
            self.other_calls.load(Ordering::Relaxed),
            self.other_nanos.load(Ordering::Relaxed),
        )
    }

    pub(crate) fn record(&self, call: WriteCall) {
        if call.commit() {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(call);
        } else {
            self.other_calls.fetch_add(1, Ordering::Relaxed);
            self.other_nanos.fetch_add(call.nanos(), Ordering::Relaxed);
        }
    }
}
