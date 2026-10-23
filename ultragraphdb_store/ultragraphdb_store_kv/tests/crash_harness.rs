/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills the crash writer (`src/bin/kv_crash_writer.rs`) at a point it announces and checks what its store holds
//! after reopening. Shared by the crash test and the kv-store scenarios.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use fjall::{Database, KeyspaceCreateOptions, Readable};
use ultragraphdb_store_kv::{DATA_KEYSPACE, Durability, FjallStore, META_KEYSPACE};
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

pub const KEYS_PER_BATCH: usize = 20_000;
pub const DROPPED_KEYS: usize = 60_000;
pub const DROP_PREFIX: &[u8] = b"\x00dropme";

/// The writer binary: Bazel passes its runfiles path in `KV_CRASH_WRITER`, Cargo builds it beside the test.
fn writer() -> PathBuf {
    std::env::var_os("KV_CRASH_WRITER")
        .map(PathBuf::from)
        .or_else(|| option_env!("CARGO_BIN_EXE_kv_crash_writer").map(PathBuf::from))
        .expect("the crash writer is a data dependency of this test")
}

/// A directory of its own under Bazel's `TEST_TMPDIR`, or the system's temporary directory under Cargo.
pub fn fresh_dir(name: &str) -> PathBuf {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("kv-crash-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Runs the writer in `mode` on `dir`, waits for it to print `line`, waits `delay`, and kills it. Returns every line
/// it printed.
pub fn kill_after(dir: &Path, mode: &str, line: &str, delay: Duration) -> Vec<String> {
    let mut child: Child = Command::new(writer())
        .arg(dir)
        .arg(mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the writer starts");
    let stdout = child.stdout.take().expect("piped");
    let mut lines = BufReader::new(stdout).lines();
    let mut seen = Vec::new();
    for l in lines.by_ref() {
        let l = l.expect("the writer's output is text");
        let reached = l == line;
        seen.push(l);
        if reached {
            break;
        }
    }
    assert_eq!(
        seen.last().map(String::as_str),
        Some(line),
        "the writer stopped early: {seen:?}"
    );
    std::thread::sleep(delay);
    child.kill().expect("the writer is killed");
    child.wait().expect("the writer is reaped");
    // Lines it printed before the kill landed.
    seen.extend(lines.map_while(Result::ok));
    seen
}

/// How many keys of batch `i` the store holds at version `i`, and its applied timestamp.
pub fn batch_counts(store: &FjallStore, batches: u64) -> Vec<usize> {
    let snapshot = store.snapshot(u64::MAX).expect("readable");
    (1..=batches)
        .map(|i| {
            let mut prefix = b"\x00b".to_vec();
            prefix.extend_from_slice(&i.to_be_bytes());
            snapshot
                .scan(&Scan::prefix(prefix))
                .filter(|item| item.as_ref().is_ok_and(|item| item.version() == i))
                .count()
        })
        .collect()
}

/// Whether the killed writer's store still holds its drop record, and how many keys under the dropped prefix it
/// holds, read raw before the store finishes the drop.
pub fn raw_drop_state(dir: &Path) -> (bool, usize) {
    let db = Database::builder(dir).open().expect("fjall opens");
    let data = db
        .keyspace(DATA_KEYSPACE, KeyspaceCreateOptions::default)
        .expect("data");
    let meta = db
        .keyspace(META_KEYSPACE, KeyspaceCreateOptions::default)
        .expect("meta");
    let snapshot = db.snapshot();
    let record = snapshot.get(&meta, b"drop").expect("readable").is_some();
    // The stored prefix: the user key's zero bytes escaped.
    let left = snapshot.prefix(&data, b"\x00\xffdropme").count();
    (record, left)
}

pub fn open(dir: &Path) -> FjallStore {
    FjallStore::open(dir, Durability::Process).expect("the store reopens")
}

/// What a reopened store holds of the drop: keys under the prefix, the entry, the key outside it.
pub fn drop_outcome(store: &FjallStore) -> (usize, bool, bool) {
    let snapshot = store.snapshot(10).expect("readable");
    let mut initial = DROP_PREFIX.to_vec();
    initial.extend_from_slice(b"-initial");
    let under = snapshot
        .scan(&Scan::prefix(DROP_PREFIX.to_vec()))
        .filter(|i| i.is_ok())
        .count();
    let entry = snapshot.get(&initial).expect("readable").is_some();
    let kept = snapshot.get(b"\x00keep").expect("readable").is_some();
    (under, entry, kept)
}
