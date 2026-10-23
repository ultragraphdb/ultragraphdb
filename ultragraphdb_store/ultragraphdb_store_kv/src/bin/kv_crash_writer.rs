/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The crash test's writer: opens a store and writes into it until the test kills it, saying on stdout what it is
//! about to do, so that the test kills it during a batch or during a drop.
//!
//! `kv_crash_writer <dir> batches` writes batch after batch, batch `i` holding [`KEYS_PER_BATCH`] keys at version
//! `i` and recording `i` as applied. `kv_crash_writer <dir> drop` writes [`DROPPED_KEYS`] keys under
//! [`DROP_PREFIX`], syncs, then drops the prefix with one entry to write once it is empty, and waits.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

const KEYS_PER_BATCH: u32 = 20_000;
const DROPPED_KEYS: u32 = 60_000;
const DROP_PREFIX: &[u8] = b"\x00dropme";

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
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

fn drop_prefix(store: &FjallStore) -> Result<(), StoreError> {
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
    say("dropping");
    let mut initial = DROP_PREFIX.to_vec();
    initial.extend_from_slice(b"-initial");
    store.drop_prefixes(
        9,
        &[DROP_PREFIX.to_vec()],
        vec![Entry::put(initial, 1, 0x01, b"initial".to_vec())],
    )?;
    say("dropped");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(dir), Some(mode)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: kv_crash_writer <dir> batches|drop");
        return ExitCode::from(2);
    };
    let store = match FjallStore::open(PathBuf::from(dir), Durability::Process) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let result = match mode.as_str() {
        "batches" => batches(&store),
        "drop" => drop_prefix(&store),
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
