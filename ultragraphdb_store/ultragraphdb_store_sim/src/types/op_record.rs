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
        batch: WriteBatch,
        mode: PersistMode,
    },
    /// A batch refused before anything was written.
    Refused {
        batch: WriteBatch,
        error: StoreError,
    },
    /// A batch failed during its persist and poisoned the store.
    BatchFailed {
        batch: WriteBatch,
    },
    /// The process crashed while the batch was written.
    CrashedDuringBatch {
        batch: WriteBatch,
    },
    /// A write refused because the store is poisoned or its process is down: nothing but recovery may follow a
    /// failure.
    WriteWhileFailed {
        operation: &'static str,
    },
    Synced,
    /// A drop finished: of everything when `prefixes` is `None`.
    Dropped {
        ts: u64,
        prefixes: Option<Vec<Vec<u8>>>,
        entries: Vec<Entry>,
    },
    /// A crash, or a power loss when `power_loss`, stopped a drop at `stage`.
    DropInterrupted {
        ts: u64,
        prefixes: Option<Vec<Vec<u8>>>,
        entries: Vec<Entry>,
        stage: DropStage,
        power_loss: bool,
    },
    /// A rebuild record written and synced.
    RebuildRecorded {
        record: RebuildRecord,
    },
    /// A crash, or a power loss when `power_loss`, stopped the rebuild of `record` at `stage`. At `RecordWritten` the
    /// record was handed to the operating system and not synced; what a later stage wrote is logged before this.
    RebuildInterrupted {
        record: RebuildRecord,
        stage: RebuildStage,
        power_loss: bool,
    },
    TemporaryCreated {
        temporary: u64,
    },
    /// A batch written to a temporary keyspace.
    TemporaryApplied {
        temporary: u64,
        batch: WriteBatch,
    },
    /// A batch to a temporary keyspace refused before anything was written.
    TemporaryRefused {
        temporary: u64,
        batch: WriteBatch,
        error: StoreError,
    },
    /// A batch to a temporary keyspace failed and poisoned the store, or the process crashed while it was written
    /// when `crashed`.
    TemporaryFailed {
        temporary: u64,
        batch: WriteBatch,
        crashed: bool,
    },
    /// A snapshot of a temporary keyspace, reading every version it holds; its reads are logged under `id`.
    TemporarySnapshotOpened {
        id: u64,
        temporary: u64,
    },
    TemporaryDropped {
        temporary: u64,
    },
    Banned {
        namespace: u64,
    },
    DiscardSet {
        ts: u64,
    },
    HorizonSet {
        ts: u64,
    },
    Compacted,
    Swept,
    SnapshotOpened {
        id: u64,
        read_ts: u64,
    },
    SnapshotRefused {
        read_ts: u64,
        error: StoreError,
    },
    SnapshotClosed {
        id: u64,
    },
    Read {
        id: u64,
        key: Vec<u8>,
        result: Result<Option<Item>, StoreError>,
    },
    ReadMany {
        id: u64,
        keys: Vec<Vec<u8>>,
        result: Result<Vec<Option<Item>>, StoreError>,
    },
    /// A scan and every item it yields, its error last when it fails.
    Scanned {
        id: u64,
        scan: Scan,
        items: Vec<Result<Item, StoreError>>,
    },
    /// A stored version's bytes were corrupted.
    Corrupted {
        key: Vec<u8>,
        version: u64,
        corruption: Corruption,
    },
    /// An operation was delayed by `nanos` of simulated time.
    Slowed {
        nanos: u64,
    },
    Crashed,
    PowerLost,
    Recovered,
}
