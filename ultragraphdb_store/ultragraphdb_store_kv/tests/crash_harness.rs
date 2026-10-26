/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills the crash writer (`src/bin/kv_crash_writer.rs`) at a point it announces, a stage it stops at or a batch it
//! begins, and checks what its store holds after reopening. Shared by the crash test and the kv-store scenarios.

use std::io::{BufRead, BufReader, Lines};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::Duration;

use fjall::{Database, KeyspaceCreateOptions, Readable};
use ultragraphdb_store_kv::{
    DATA_KEYSPACE, Durability, FjallStore, META_KEYSPACE, TEMPORARY_KEYSPACE_PREFIX,
};
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

/// The stages the writer's drop mode announces, in order: before the drop, with its record synced, after each batch
/// of removals, with its data removed, with its entries synced, and after the drop returned.
pub const DROP_STAGES: [&str; 11] = [
    "start",
    "record-synced",
    "removing-10000",
    "removing-20000",
    "removing-30000",
    "removing-40000",
    "removing-50000",
    "removing-60000",
    "data-removed",
    "entries-synced",
    "dropped",
];
/// The stages the writer's temporary mode announces, in order.
pub const TEMPORARY_STAGES: [&str; 3] = ["data-synced", "temporary-created", "temporary-written"];
/// The stages the writer's record mode announces, in order.
pub const RECORD_STAGES: [&str; 2] = ["start", "record-synced"];

/// The writer's stdout, line by line.
type Output = Lines<BufReader<ChildStdout>>;

/// Runs the writer with `args` on `dir` and reads its stdout until it prints `line`. Returns the child, the rest of
/// its stdout and the lines read, the last of which is `line`.
fn run_until(dir: &Path, args: &[&str], line: &str) -> (Child, Output, Vec<String>) {
    let mut child: Child = Command::new(writer())
        .arg(dir)
        .args(args)
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
    (child, lines, seen)
}

/// Runs the writer in `mode` on `dir`, waits for it to print `line`, waits `delay`, and kills it. Returns every line
/// it printed.
pub fn kill_after(dir: &Path, mode: &str, line: &str, delay: Duration) -> Vec<String> {
    let (mut child, lines, mut seen) = run_until(dir, &[mode], line);
    std::thread::sleep(delay);
    child.kill().expect("the writer is killed");
    child.wait().expect("the writer is reaped");
    // Lines it printed before the kill landed.
    seen.extend(lines.map_while(Result::ok));
    seen
}

/// Runs the writer in `mode` on `dir`, stopping at `stage`: it announces `stage <stage>` and waits there, and is
/// killed once the line is read, so the kill lands at that stage and nowhere else. Returns every line it printed.
pub fn kill_at(dir: &Path, mode: &str, stage: &str) -> Vec<String> {
    let (mut child, _, seen) = run_until(dir, &[mode, stage], &format!("stage {stage}"));
    child.kill().expect("the writer is killed");
    child.wait().expect("the writer is reaped");
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

/// For each of the first `batches` batches of the `mixed` mode, how many of its keys without a version the store
/// holds. The scenarios use it; the crash test does not.
#[allow(dead_code)]
pub fn versionless_counts(store: &FjallStore, batches: u64) -> Vec<usize> {
    let held = store.dump_versionless().expect("readable");
    (1..=batches)
        .map(|i| {
            let mut prefix = b"t".to_vec();
            prefix.extend_from_slice(&i.to_be_bytes());
            held.iter().filter(|(k, _)| k.starts_with(&prefix)).count()
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

/// How many temporary keyspaces the killed writer's store holds, read raw before the store drops them.
pub fn raw_temporary_keyspaces(dir: &Path) -> usize {
    let db = Database::builder(dir).open().expect("fjall opens");
    db.list_keyspace_names()
        .iter()
        .filter(|n| n.starts_with(TEMPORARY_KEYSPACE_PREFIX))
        .count()
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
