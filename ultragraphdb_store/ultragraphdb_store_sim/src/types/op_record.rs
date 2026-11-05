/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Entry, Item, PersistMode, RebuildRecord, Scan, WriteBatch};

use crate::types::corruption::Corruption;
use crate::types::drop_stage::DropStage;
use crate::types::rebuild_stage::RebuildStage;

/// One operation at the simulated store's boundary, as the store logs it: what the caller asked for and what the
/// store answered, and every fault the store injected. The shadow model checks the store against this log, and a
/// run's trace is made of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpRecord {
    /// A batch written.
    Applied {
        /// The batch.
        batch: WriteBatch,
        /// How it was persisted.
        mode: PersistMode,
    },
    /// A batch refused before anything was written.
    Refused {
        /// The batch.
        batch: WriteBatch,
        /// Why it was refused.
        error: StoreError,
    },
    /// A batch failed during its persist and poisoned the store.
    BatchFailed {
        /// The batch.
        batch: WriteBatch,
    },
    /// The process crashed while the batch was written.
    CrashedDuringBatch {
        /// The batch being written.
        batch: WriteBatch,
    },
    /// A write refused because the store is poisoned or its process is down: nothing but recovery may follow a
    /// failure.
    WriteWhileFailed {
        /// The operation refused.
        operation: &'static str,
    },
    /// The store synced what it had written.
    Synced,
    /// A drop finished: of everything when `prefixes` is `None`.
    Dropped {
        /// The timestamp of the drop.
        ts: u64,
        /// The prefixes dropped; `None` for everything.
        prefixes: Option<Vec<Vec<u8>>>,
        /// The entries the drop wrote.
        entries: Vec<Entry>,
    },
    /// A crash, or a power loss when `power_loss`, stopped a drop at `stage`.
    DropInterrupted {
        /// The timestamp of the drop.
        ts: u64,
        /// The prefixes dropped; `None` for everything.
        prefixes: Option<Vec<Vec<u8>>>,
        /// The entries the drop was to write.
        entries: Vec<Entry>,
        /// The stage the drop stopped at.
        stage: DropStage,
        /// Whether the power was lost, rather than the process crashing.
        power_loss: bool,
    },
    /// A rebuild record written and synced.
    RebuildRecorded {
        /// The record.
        record: RebuildRecord,
    },
    /// A crash, or a power loss when `power_loss`, stopped the rebuild of `record` at `stage`. At `RecordWritten` the
    /// record was handed to the operating system and not synced; what a later stage wrote is logged before this.
    RebuildInterrupted {
        /// The record of the rebuild.
        record: RebuildRecord,
        /// The stage the rebuild stopped at.
        stage: RebuildStage,
        /// Whether the power was lost, rather than the process crashing.
        power_loss: bool,
    },
    /// A temporary keyspace created.
    TemporaryCreated {
        /// The temporary keyspace's number.
        temporary: u64,
    },
    /// A batch written to a temporary keyspace.
    TemporaryApplied {
        /// The temporary keyspace's number.
        temporary: u64,
        /// The batch.
        batch: WriteBatch,
    },
    /// A batch to a temporary keyspace refused before anything was written.
    TemporaryRefused {
        /// The temporary keyspace's number.
        temporary: u64,
        /// The batch.
        batch: WriteBatch,
        /// Why it was refused.
        error: StoreError,
    },
    /// A batch to a temporary keyspace failed and poisoned the store, or the process crashed while it was written
    /// when `crashed`.
    TemporaryFailed {
        /// The temporary keyspace's number.
        temporary: u64,
        /// The batch.
        batch: WriteBatch,
        /// Whether the process crashed, rather than the batch failing.
        crashed: bool,
    },
    /// A snapshot of a temporary keyspace, reading every version it holds; its reads are logged under `id`.
    TemporarySnapshotOpened {
        /// The snapshot's number.
        id: u64,
        /// The temporary keyspace's number.
        temporary: u64,
    },
    /// A temporary keyspace dropped with what it held.
    TemporaryDropped {
        /// The temporary keyspace's number.
        temporary: u64,
    },
    /// A tenant banned.
    Banned {
        /// The tenant.
        tenant: u64,
    },
    /// The discard timestamp set.
    DiscardSet {
        /// The discard timestamp.
        ts: u64,
    },
    /// The tombstone horizon set.
    HorizonSet {
        /// The tombstone horizon.
        ts: u64,
    },
    /// A compaction under the discard timestamp.
    Compacted,
    /// A sweep under the tombstone horizon.
    Swept,
    /// A snapshot opened; its reads are logged under `id`.
    SnapshotOpened {
        /// The snapshot's number.
        id: u64,
        /// The timestamp it reads at.
        read_ts: u64,
    },
    /// A snapshot refused.
    SnapshotRefused {
        /// The timestamp it was to read at.
        read_ts: u64,
        /// Why it was refused.
        error: StoreError,
    },
    /// A snapshot closed.
    SnapshotClosed {
        /// The snapshot's number.
        id: u64,
    },
    /// A point read through snapshot `id`.
    Read {
        /// The snapshot's number.
        id: u64,
        /// The key read.
        key: Vec<u8>,
        /// What the read answered.
        result: Result<Option<Item>, StoreError>,
    },
    /// A read of the keyspace without versions, through snapshot `id`, or through the store when `None`.
    ReadVersionless {
        /// The snapshot's number; `None` for the store.
        id: Option<u64>,
        /// The key read.
        key: Vec<u8>,
        /// What the read answered.
        result: Result<Option<Vec<u8>>, StoreError>,
    },
    /// A read of several keys through snapshot `id`.
    ReadMany {
        /// The snapshot's number.
        id: u64,
        /// The keys read, in order.
        keys: Vec<Vec<u8>>,
        /// What the read answered, one item per key.
        result: Result<Vec<Option<Item>>, StoreError>,
    },
    /// A scan and every item it yields, its error last when it fails.
    Scanned {
        /// The snapshot's number.
        id: u64,
        /// The scan.
        scan: Scan,
        /// Every item it yielded, in order.
        items: Vec<Result<Item, StoreError>>,
    },
    /// A stored version's bytes were corrupted.
    Corrupted {
        /// The key.
        key: Vec<u8>,
        /// The version.
        version: u64,
        /// How its bytes were corrupted.
        corruption: Corruption,
    },
    /// An operation was delayed by `nanos` of simulated time.
    Slowed {
        /// The delay, in nanoseconds.
        nanos: u64,
    },
    /// The process crashed.
    Crashed,
    /// The power was lost.
    PowerLost,
    /// The store recovered and reopened.
    Recovered,
}
