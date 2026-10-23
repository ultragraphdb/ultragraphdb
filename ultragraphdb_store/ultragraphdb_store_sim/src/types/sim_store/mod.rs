/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod corrupted;
mod fault_injector;
mod gc;
mod record;
mod sim_store_store;
pub(crate) mod state;

use std::collections::BTreeMap;
use std::mem;
use std::sync::{Arc, Mutex};

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Entry, key_namespace};

use crate::types::contents::Contents;
use crate::types::corruption::Corruption;
use crate::types::fault::Fault;
use crate::types::fault_config::FaultConfig;
use crate::types::op_record::OpRecord;
use crate::types::seed::Seed;
use crate::types::sim_clock::SimClock;
use crate::types::stored_version::StoredVersion;
use crate::types::stream::Stream;
use crate::utils::lock::lock;

pub(crate) use crate::types::sim_store::corrupted::Corrupted;
use crate::types::sim_store::fault_injector::FaultInjector;
use crate::types::sim_store::record::Record;
use crate::types::sim_store::state::{DropRecord, State, decode};

/// The removals one batch of a drop holds; a drop removes its data in as many batches as it needs.
const REMOVALS_PER_BATCH: usize = 64;

/// The in-memory [`ultragraphdb_traits_store::Store`] of the simulator. It keeps a journal of what it wrote since
/// its last sync and injects the faults its stream draws, or those forced on it with [`SimStore::inject`]: a
/// failed batch, which poisons it until [`SimStore::recover`]; a crash while a batch is written, which may lose
/// that batch; a power loss, which keeps a prefix of the batches written since the last sync; a crash or power
/// loss during a drop; failed reads, at once or partway through a scan; corrupted values; slow operations.
///
/// Every operation and every fault is logged ([`SimStore::take_log`]) for the shadow model, and
/// [`SimStore::contents`] shows what the store holds, read without faults. Clones share one store.
#[derive(Debug, Clone)]
pub struct SimStore {
    shared: Arc<Mutex<Inner>>,
    clock: SimClock,
}

/// Whether the store serves, and what a recovery decides when it does not.
#[derive(Debug, Clone)]
pub(crate) enum Status {
    Open,
    /// A batch failed during its persist; it is absent now, and present or absent after recovery.
    Poisoned {
        failed: Record,
    },
    /// The process is down: crashed, or its power lost, possibly while `in_flight` was written.
    Down {
        power_loss: bool,
        in_flight: Option<Record>,
    },
}

#[derive(Debug)]
pub(crate) struct Inner {
    /// What the process sees.
    pub current: Arc<State>,
    /// What survives a power loss: the state at the last sync.
    durable: Arc<State>,
    /// The journal records written since the last sync, in order.
    pending: Vec<Record>,
    pub status: Status,
    /// The read timestamps of the open snapshots, with how many are open at each.
    pub readers: BTreeMap<u64, usize>,
    /// Incremented at every recovery, so that a snapshot of an earlier process releases nothing.
    pub generation: u64,
    pub next_snapshot: u64,
    pub faults: FaultInjector,
    pub corrupted: Corrupted,
    pub log: Vec<OpRecord>,
    pub clock: SimClock,
}

impl SimStore {
    /// An empty store whose faults are drawn from `seed`'s fault stream as `faults` configures them, and whose slow
    /// operations advance `clock`.
    pub fn new(seed: Seed, faults: FaultConfig, clock: SimClock) -> Self {
        let inner = Inner {
            current: Arc::default(),
            durable: Arc::default(),
            pending: Vec::new(),
            status: Status::Open,
            readers: BTreeMap::new(),
            generation: 0,
            next_snapshot: 0,
            faults: FaultInjector::new(seed.stream(Stream::Faults), faults),
            corrupted: Corrupted::default(),
            log: Vec::new(),
            clock: clock.clone(),
        };
        Self {
            shared: Arc::new(Mutex::new(inner)),
            clock,
        }
    }

    pub fn clock(&self) -> &SimClock {
        &self.clock
    }

    /// Forces `fault` on the next operation it applies to.
    pub fn inject(&self, fault: Fault) {
        lock(&self.shared).faults.force(fault);
    }

    /// Corrupts the stored bytes of `key` at `version`, as every later read finds them.
    pub fn corrupt(&self, key: &[u8], version: u64, corruption: Corruption) {
        let mut inner = lock(&self.shared);
        inner.corrupted.insert(key.to_vec(), version, corruption);
        inner.log.push(OpRecord::Corrupted {
            key: key.to_vec(),
            version,
            corruption,
        });
    }

    /// Crashes the process: what was handed to the operating system survives.
    pub fn crash(&self) {
        let mut inner = lock(&self.shared);
        inner.status = match mem::replace(&mut inner.status, Status::Open) {
            Status::Open => Status::Down {
                power_loss: false,
                in_flight: None,
            },
            Status::Poisoned { failed } => Status::Down {
                power_loss: false,
                in_flight: Some(failed),
            },
            down => down,
        };
        inner.log.push(OpRecord::Crashed);
    }

    /// Cuts the power: of what was written since the last sync, a prefix survives.
    pub fn power_loss(&self) {
        let mut inner = lock(&self.shared);
        inner.status = match mem::replace(&mut inner.status, Status::Open) {
            Status::Open => Status::Down {
                power_loss: true,
                in_flight: None,
            },
            Status::Poisoned { failed } => Status::Down {
                power_loss: true,
                in_flight: Some(failed),
            },
            Status::Down { in_flight, .. } => Status::Down {
                power_loss: true,
                in_flight,
            },
        };
        inner.log.push(OpRecord::PowerLost);
    }

    /// Whether the store is poisoned or its process down, so that recovery is the only next step.
    pub fn needs_recovery(&self) -> bool {
        !matches!(lock(&self.shared).status, Status::Open)
    }

    /// Reopens the store, as a restarted process does. After a power loss, a prefix of the journal records since the
    /// last sync survives, its length drawn from the fault stream; otherwise every record survives, and a batch that
    /// failed or was being written is present or absent, as the stream decides. A drop whose record survived is then
    /// finished. Snapshots opened before no longer hold the discard timestamp.
    pub fn recover(&self) {
        let mut inner = lock(&self.shared);
        let (power_loss, extra) = match mem::replace(&mut inner.status, Status::Open) {
            Status::Open => (false, None),
            Status::Poisoned { failed } => (false, Some(failed)),
            Status::Down {
                power_loss,
                in_flight,
            } => (power_loss, in_flight),
        };
        let mut records = mem::take(&mut inner.pending);
        if power_loss {
            records.extend(extra);
            let survivors = inner.faults.rng.index(records.len() + 1);
            records.truncate(survivors);
        } else if let Some(record) = extra
            && inner.faults.rng.chance(500)
        {
            records.push(record);
        }
        let mut state = (*inner.durable).clone();
        records.iter().for_each(|r| r.apply(&mut state));
        inner.current = Arc::new(state);
        if power_loss {
            inner.durable = inner.current.clone();
        } else {
            inner.pending = records;
        }
        inner.readers.clear();
        inner.generation += 1;
        if let Some(record) = inner.current.drop_record.clone() {
            inner.finish_drop(&record);
        }
        inner.log.push(OpRecord::Recovered);
    }

    /// Everything the store holds, read without faults.
    pub fn contents(&self) -> Contents {
        let inner = lock(&self.shared);
        let state = &inner.current;
        let mut keys: BTreeMap<Vec<u8>, Vec<StoredVersion>> = BTreeMap::new();
        for ((key, std::cmp::Reverse(version)), stored) in &state.data {
            let bytes = inner.corrupted.bytes(key, *version, stored);
            let version = match decode(&bytes) {
                Ok(d) => StoredVersion::new(*version, d.meta, d.discard, d.deletion, d.value),
                Err(_) => StoredVersion::undecodable(*version, bytes),
            };
            keys.entry(key.clone()).or_default().push(version);
        }
        Contents::new(
            keys,
            state.applied_ts,
            state.banned.clone(),
            state.discard_ts,
            state.horizon,
        )
    }

    /// The log of operations and faults since the last call, oldest first.
    pub fn take_log(&self) -> Vec<OpRecord> {
        mem::take(&mut lock(&self.shared).log)
    }

    pub(crate) fn shared(&self) -> &Arc<Mutex<Inner>> {
        &self.shared
    }
}

impl Inner {
    /// The discard timestamp in effect: the one asked for, held at or below the oldest open snapshot.
    pub fn discard_ts(&self) -> u64 {
        let oldest = self.readers.keys().next().copied().unwrap_or(u64::MAX);
        self.current.discard_ts.min(oldest)
    }

    /// The tombstone horizon in effect.
    pub fn tombstone_horizon(&self) -> u64 {
        let discard = self.discard_ts();
        self.current.horizon.unwrap_or(discard).min(discard)
    }

    /// Writes `record` to the journal and to what the process sees.
    pub fn write(&mut self, record: Record) {
        if let Record::Batch { entries, .. } = &record {
            entries
                .iter()
                .for_each(|e| self.corrupted.remove(e.key(), e.version()));
        }
        record.apply(Arc::make_mut(&mut self.current));
        self.pending.push(record);
    }

    pub fn sync(&mut self) {
        self.durable = self.current.clone();
        self.pending.clear();
    }

    /// Advances the clock when the fault stream makes this operation slow.
    pub fn maybe_slow(&mut self) {
        if let Some(nanos) = self.faults.slow() {
            self.clock.advance(nanos);
            self.log.push(OpRecord::Slowed { nanos });
        }
    }

    /// The batch record of `entries`, those addressed to a banned namespace dropped.
    pub fn batch_record(&self, entries: Vec<Entry>, applied_ts: Option<u64>) -> Record {
        let banned = &self.current.banned;
        let entries = entries
            .into_iter()
            .filter(|e| key_namespace(e.key()).is_none_or(|ns| !banned.contains(&ns)))
            .collect();
        Record::Batch {
            entries,
            applied_ts,
        }
    }

    /// Refuses a write while the store is poisoned or down.
    pub fn refuse_unless_open(&mut self, operation: &'static str) -> Result<(), StoreError> {
        if matches!(self.status, Status::Open) {
            return Ok(());
        }
        self.log.push(OpRecord::WriteWhileFailed { operation });
        Err(StoreError::Poisoned())
    }

    /// Removes what `record` covers and writes its entries, from wherever the drop had come.
    pub fn finish_drop(&mut self, record: &DropRecord) {
        self.remove_dropped(record);
        let entries = self.batch_record(record.entries.clone(), None);
        self.write(entries);
        self.sync();
        self.write(Record::EndDrop);
    }

    /// The removals of a drop, in batches, and for a drop of everything the clear and the metadata reset.
    pub fn remove_dropped(&mut self, record: &DropRecord) {
        if record.prefixes.is_none() {
            self.write(Record::Clear);
            self.write(Record::ResetMeta);
            self.corrupted = Corrupted::default();
            return;
        }
        let removed: Vec<(Vec<u8>, u64)> = self
            .current
            .data
            .keys()
            .filter(|(key, _)| record.covers(key))
            .map(|(key, std::cmp::Reverse(v))| (key.clone(), *v))
            .collect();
        removed
            .iter()
            .for_each(|(key, v)| self.corrupted.remove(key, *v));
        removed
            .chunks(REMOVALS_PER_BATCH)
            .for_each(|chunk| self.write(Record::Remove(chunk.to_vec())));
    }
}
