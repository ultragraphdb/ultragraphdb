/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A store that stops its process at a chosen stage of a rebuild: it announces the stage on stdout and waits to be
//! killed. What the kill test on fjall runs its writer over, so that the kill lands at each stage.

use std::io::Write;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Entry, PersistMode, RebuildRecord, Store, Temporary, WriteBatch};

/// The stages a rebuild on fjall is killed at: before its record, with its record synced, with a batch written to its
/// temporary keyspace, and with its swap batch written.
pub const KILL_STAGES: [&str; 4] = [
    "start",
    "record-synced",
    "temporary-written",
    "record-removed",
];

/// Says `line` on stdout.
pub fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Says `stage <stage>` and waits for the kill.
fn stop(stage: &str) -> ! {
    say(&format!("stage {stage}"));
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// `inner`, stopping the process when a rebuild reaches `stage`.
#[derive(Debug)]
pub struct KillPoint<S> {
    inner: S,
    stage: String,
}

impl<S> KillPoint<S> {
    pub fn new(inner: S, stage: &str) -> Self {
        Self {
            inner,
            stage: stage.to_string(),
        }
    }

    fn at(&self, stage: &str) {
        if self.stage == stage {
            stop(stage);
        }
    }
}

impl<S: Store> Store for KillPoint<S> {
    type Snapshot = S::Snapshot;

    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError> {
        let removes = !batch.removed_rebuild_records().is_empty();
        self.inner.apply(batch, mode)?;
        if removes {
            self.at("record-removed");
        }
        Ok(())
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        self.inner.snapshot(read_ts)
    }

    fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), StoreError> {
        self.inner.drop_prefixes(ts, prefixes, entries)
    }

    fn drop_all(&self, entries: Vec<Entry>) -> Result<(), StoreError> {
        self.inner.drop_all(entries)
    }

    fn ban_namespace(&self, namespace: u64) -> Result<(), StoreError> {
        self.inner.ban_namespace(namespace)
    }

    fn set_discard_ts(&self, ts: u64) -> Result<(), StoreError> {
        self.inner.set_discard_ts(ts)
    }

    fn discard_ts(&self) -> u64 {
        self.inner.discard_ts()
    }

    fn set_tombstone_horizon(&self, ts: u64) -> Result<(), StoreError> {
        self.inner.set_tombstone_horizon(ts)
    }

    fn tombstone_horizon(&self) -> u64 {
        self.inner.tombstone_horizon()
    }

    fn applied_ts(&self) -> u64 {
        self.inner.applied_ts()
    }

    fn compact(&self) -> Result<(), StoreError> {
        self.inner.compact()
    }

    fn sweep_deletions(&self) -> Result<(), StoreError> {
        self.inner.sweep_deletions()
    }

    fn sync(&self) -> Result<(), StoreError> {
        self.inner.sync()
    }

    fn create_temporary(&self) -> Result<Temporary, StoreError> {
        self.inner.create_temporary()
    }

    fn apply_temporary(&self, temporary: &Temporary, batch: WriteBatch) -> Result<(), StoreError> {
        self.inner.apply_temporary(temporary, batch)?;
        self.at("temporary-written");
        Ok(())
    }

    fn temporary_snapshot(&self, temporary: &Temporary) -> Result<Self::Snapshot, StoreError> {
        self.inner.temporary_snapshot(temporary)
    }

    fn drop_temporary(&self, temporary: Temporary) -> Result<(), StoreError> {
        self.inner.drop_temporary(temporary)
    }

    fn write_rebuild_record(&self, record: &RebuildRecord) -> Result<(), StoreError> {
        self.at("start");
        self.inner.write_rebuild_record(record)?;
        self.at("record-synced");
        Ok(())
    }

    fn rebuild_records(&self) -> Result<Vec<RebuildRecord>, StoreError> {
        self.inner.rebuild_records()
    }
}
