/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The crash test's writer: opens a store and writes into it until the test kills it, saying on stdout what it is
//! about to do, so that the test kills it during a batch or at a stage it stops at.
//!
//! `kv_crash_writer <dir> batches` writes batch after batch, batch `i` holding [`KEYS_PER_BATCH`] keys at version
//! `i` and recording `i` as applied. The other modes take the stage to stop at, `kv_crash_writer <dir> <mode>
//! <stage>`: each announces `stage <name>` on stdout at every stage it reaches, and at `<stage>` waits to be killed,
//! as it waits after its last. `drop` writes [`DROPPED_KEYS`] keys under [`DROP_PREFIX`], syncs, then drops the
//! prefix with one entry to write once it is empty, through the stages `start`, `record-synced`, `removing-<n>` after
//! each batch of removals, `data-removed`, `entries-synced` and `dropped`. `temporary` writes and syncs one batch at
//! version 1 (`data-synced`), creates a temporary keyspace (`temporary-created`) and writes [`TEMPORARY_KEYS`] keys to
//! it (`temporary-written`). `record` writes rebuild record 7, between `start` and `record-synced`.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{DropStage, Durability, FjallStore};
use ultragraphdb_traits_store::{Entry, PersistMode, RebuildRecord, Store, WriteBatch};

const KEYS_PER_BATCH: u32 = 20_000;
const DROPPED_KEYS: u32 = 60_000;
const DROP_PREFIX: &[u8] = b"\x00dropme";
/// The keys the temporary mode writes to its temporary keyspace.
const TEMPORARY_KEYS: u32 = 1_000;

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Announces `name` and, when it is the stage to stop at, waits to be killed there.
fn stage(name: &str, stop: &str) {
    say(&format!("stage {name}"));
    if name == stop {
        wait();
    }
}

fn key(prefix: &[u8], i: u64, j: u32) -> Vec<u8> {
    let mut key = prefix.to_vec();
    key.extend_from_slice(&i.to_be_bytes());
    key.extend_from_slice(&j.to_be_bytes());
    key
}

fn batches(store: &FjallStore) -> Result<(), StoreError> {
    for i in 1u64.. {
        let mut batch = WriteBatch::new();
        (0..KEYS_PER_BATCH)
            .for_each(|j| batch.push(Entry::put(key(b"\x00b", i, j), i, 0x04, vec![7; 100])));
        batch.set_applied_ts(i);
        say(&format!("batch {i}"));
        store.apply(batch, PersistMode::Buffer)?;
        say(&format!("applied {i}"));
    }
    Ok(())
}

fn drop_prefix(store: &FjallStore, stop: &str) -> Result<(), StoreError> {
    for chunk in 0..(DROPPED_KEYS / KEYS_PER_BATCH) {
        let mut batch = WriteBatch::new();
        (0..KEYS_PER_BATCH).for_each(|j| {
            batch.push(Entry::put(
                key(DROP_PREFIX, u64::from(chunk), j),
                2,
                0x04,
                vec![1],
            ))
        });
        store.apply(batch, PersistMode::Buffer)?;
    }
    let mut keep = WriteBatch::new();
    keep.push(Entry::put(b"\x00keep".to_vec(), 2, 0x04, vec![1]));
    store.apply(keep, PersistMode::Sync)?;
    stage("start", stop);
    let mut initial = DROP_PREFIX.to_vec();
    initial.extend_from_slice(b"-initial");
    store.drop_prefixes_observed(
        9,
        &[DROP_PREFIX.to_vec()],
        vec![Entry::put(initial, 1, 0x01, b"initial".to_vec())],
        |reached| match reached {
            DropStage::RecordSynced => stage("record-synced", stop),
            DropStage::Removing(n) => stage(&format!("removing-{n}"), stop),
            DropStage::DataRemoved => stage("data-removed", stop),
            DropStage::EntriesSynced => stage("entries-synced", stop),
        },
    )?;
    stage("dropped", stop);
    wait()
}

fn wait() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn temporary(store: &FjallStore, stop: &str) -> Result<(), StoreError> {
    let mut data = WriteBatch::new();
    data.push(Entry::put(b"\x00keep".to_vec(), 1, 0x04, vec![1]));
    data.set_applied_ts(1);
    store.apply(data, PersistMode::Sync)?;
    stage("data-synced", stop);
    let t = store.create_temporary()?;
    stage("temporary-created", stop);
    let mut batch = WriteBatch::new();
    (0..TEMPORARY_KEYS)
        .for_each(|j| batch.push(Entry::put(key(b"\x00t", 0, j), 1, 0x04, vec![2; 100])));
    store.apply_temporary(&t, batch)?;
    stage("temporary-written", stop);
    wait()
}

fn record(store: &FjallStore, stop: &str) -> Result<(), StoreError> {
    stage("start", stop);
    store.write_rebuild_record(&RebuildRecord::new(7, vec![b"\x00idx".to_vec()]))?;
    stage("record-synced", stop);
    wait()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(dir), Some(mode)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: kv_crash_writer <dir> batches | <dir> drop|temporary|record [<stage>]");
        return ExitCode::from(2);
    };
    let store = match FjallStore::open(PathBuf::from(dir), Durability::Process) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    // No stage named: run to the last and wait there.
    let stop = args.get(3).map_or("", String::as_str);
    let result = match mode.as_str() {
        "batches" => batches(&store),
        "drop" => drop_prefix(&store, stop),
        "temporary" => temporary(&store, stop),
        "record" => record(&store, stop),
        _ => {
            eprintln!("unknown mode {mode}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
