/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills the crash writer (`src/bin/kv_crash_writer.rs`) at a point it announces, a stage it stops at or a batch it
//! begins, and checks what its store holds after reopening. Shared by the crash test and the kv-store scenarios.

use std::io::{BufRead, BufReader, Lines};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::time::Duration;

use fjall::{Database, KeyspaceCreateOptions, Readable};
use ultragraphdb_store_kv::{
    DATA_KEYSPACE, Durability, FjallStore, META_KEYSPACE, TEMPORARY_KEYSPACE_PREFIX,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

/// How many keys each batch of the writer's batch modes writes.
pub const KEYS_PER_BATCH: usize = 20_000;
/// How many keys the writer's drop mode writes under [`DROP_PREFIX`] before it drops them.
pub const DROPPED_KEYS: usize = 60_000;
/// The prefix the writer's drop mode drops.
pub const DROP_PREFIX: &[u8] = b"\x00dropme";

/// The writer binary: Bazel passes its runfiles path in `KV_CRASH_WRITER`, Cargo builds it beside the test, in the
/// profile directory above the test's own `deps/`. `CARGO_BIN_EXE_kv_crash_writer` is set only for the tests of the
/// package owning the binary, and this crate is not that package, so it would never be read here.
fn writer() -> TestResult<PathBuf> {
    std::env::var_os("KV_CRASH_WRITER")
        .map(PathBuf::from)
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            let profile = exe.parent()?.parent()?;
            Some(profile.join("kv_crash_writer")).filter(|path| path.is_file())
        })
        .context("the crash writer is a data dependency of this test")
}

/// A directory of its own under Bazel's `TEST_TMPDIR`, or the system's temporary directory under Cargo.
pub fn fresh_dir(name: &str) -> PathBuf {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("kv-crash-{name}-{}", std::process::id()));
    // A directory an earlier run left behind, which is usually absent.
    drop(std::fs::remove_dir_all(&dir));
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
/// its stdout and the lines read, the last of which is `line`; a [`TestError`] when the writer does not start, its
/// output is not text, or it stops before printing `line`: it exits, or it prints `waits`, the line after which it
/// waits to be killed, saying nothing more. A writer stopped early is killed and reaped.
fn run_until(
    dir: &Path,
    args: &[&str],
    line: &str,
    waits: Option<&str>,
) -> TestResult<(Child, Output, Vec<String>)> {
    let mut child: Child = Command::new(writer()?)
        .arg(dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("the writer starts")?;
    let stdout = child.stdout.take().context("piped")?;
    let mut lines = BufReader::new(stdout).lines();
    let mut seen = Vec::new();
    // Ends at `line`, at the line after which the writer waits, or at the end of the writer's output, when it exits.
    for l in lines.by_ref() {
        let l = l.context("the writer's output is text")?;
        let reached = l == line || waits == Some(l.as_str());
        seen.push(l);
        if reached {
            break;
        }
    }
    if seen.last().map(String::as_str) != Some(line) {
        // A writer waiting to be killed; one that exited is reaped all the same.
        drop(child.kill());
        drop(child.wait());
        return Err(TestError::new(format!(
            "the writer stopped early: {seen:?}"
        )));
    }
    Ok((child, lines, seen))
}

/// Runs the writer in `mode` on `dir`, waits for it to print `line`, waits `delay`, and kills it. Returns every line
/// it printed.
///
/// # Errors
///
/// A [`TestError`] when the writer does not run to `line`, or is not killed and reaped.
pub fn kill_after(dir: &Path, mode: &str, line: &str, delay: Duration) -> TestResult<Vec<String>> {
    let (mut child, lines, mut seen) = run_until(dir, &[mode], line, None)?;
    std::thread::sleep(delay);
    child.kill().context("the writer is killed")?;
    let _status: ExitStatus = child.wait().context("the writer is reaped")?;
    // Lines it printed before the kill landed; the output ends, since the writer is reaped.
    seen.extend(lines.map_while(Result::ok));
    Ok(seen)
}

/// Runs the writer in `mode` on `dir`, stopping at `stage`: it announces `stage <stage>` and waits there, and is
/// killed once the line is read, so the kill lands at that stage and nowhere else. Returns every line it printed.
///
/// # Errors
///
/// A [`TestError`] when the writer does not run to `stage`, or is not killed and reaped.
pub fn kill_at(dir: &Path, mode: &str, stage: &str) -> TestResult<Vec<String>> {
    // The last stage of each mode that stops: the writer waits there whatever stage it was asked to stop at.
    let last = match mode {
        "drop" => DROP_STAGES.last(),
        "temporary" => TEMPORARY_STAGES.last(),
        "record" => RECORD_STAGES.last(),
        _ => None,
    };
    let waits = last.map(|last| format!("stage {last}"));
    let (mut child, _, seen) = run_until(
        dir,
        &[mode, stage],
        &format!("stage {stage}"),
        waits.as_deref(),
    )?;
    child.kill().context("the writer is killed")?;
    let _status: ExitStatus = child.wait().context("the writer is reaped")?;
    Ok(seen)
}

/// How many keys of batch `i` the store holds at version `i`.
///
/// # Errors
///
/// A [`TestError`] when the snapshot does not open or a read of a batch's keys fails: a failed read is not a key the
/// store lacks.
pub fn batch_counts(store: &FjallStore, batches: u64) -> TestResult<Vec<usize>> {
    let snapshot = store.snapshot(u64::MAX).context("readable")?;
    (1..=batches)
        .map(|i| {
            let mut prefix = b"\x00b".to_vec();
            prefix.extend_from_slice(&i.to_be_bytes());
            snapshot
                .scan(&Scan::prefix(prefix))
                .try_fold(0, |n, item| {
                    item.map(|item| n + usize::from(item.version() == i))
                })
                .context("a batch's keys read")
        })
        .collect()
}

/// For each of the first `batches` batches of the `mixed` mode, how many of its keys without a version the store
/// holds. The scenarios use it; the crash test does not.
///
/// # Errors
///
/// A [`TestError`] when the store does not dump its versionless keyspace.
pub fn versionless_counts(store: &FjallStore, batches: u64) -> TestResult<Vec<usize>> {
    let held = store.dump_versionless().context("readable")?;
    Ok((1..=batches)
        .map(|i| {
            let mut prefix = b"t".to_vec();
            prefix.extend_from_slice(&i.to_be_bytes());
            held.iter().filter(|(k, _)| k.starts_with(&prefix)).count()
        })
        .collect())
}

/// Whether the killed writer's store still holds its drop record, and how many keys under the dropped prefix it
/// holds, read raw before the store finishes the drop.
///
/// # Errors
///
/// A [`TestError`] when fjall does not open or a read fails.
pub fn raw_drop_state(dir: &Path) -> TestResult<(bool, usize)> {
    let db = Database::builder(dir).open().context("fjall opens")?;
    let data = db
        .keyspace(DATA_KEYSPACE, KeyspaceCreateOptions::default)
        .context("data")?;
    let meta = db
        .keyspace(META_KEYSPACE, KeyspaceCreateOptions::default)
        .context("meta")?;
    let snapshot = db.snapshot();
    let record = snapshot.get(&meta, b"drop").context("readable")?.is_some();
    // The stored prefix: the user key's zero byte written as a run of one followed by a non-zero byte.
    let left = snapshot.prefix(&data, b"\x00\xfedropme").count();
    Ok((record, left))
}

/// How many temporary keyspaces the killed writer's store holds, read raw before the store drops them.
///
/// # Errors
///
/// A [`TestError`] when fjall does not open.
pub fn raw_temporary_keyspaces(dir: &Path) -> TestResult<usize> {
    let db = Database::builder(dir).open().context("fjall opens")?;
    Ok(db
        .list_keyspace_names()
        .iter()
        .filter(|n| n.starts_with(TEMPORARY_KEYSPACE_PREFIX))
        .count())
}

/// The store in `dir`, reopened as the writer left it.
///
/// # Errors
///
/// A [`TestError`] when the store does not reopen.
pub fn open(dir: &Path) -> TestResult<FjallStore> {
    FjallStore::open(dir, Durability::Process).context("the store reopens")
}

/// What a reopened store holds of the drop: keys under the prefix, the entry, the key outside it.
///
/// # Errors
///
/// A [`TestError`] when the snapshot does not open or a read fails.
pub fn drop_outcome(store: &FjallStore) -> TestResult<(usize, bool, bool)> {
    let snapshot = store.snapshot(10).context("readable")?;
    let mut initial = DROP_PREFIX.to_vec();
    initial.extend_from_slice(b"-initial");
    // A failed read is not a key the drop removed.
    let under = snapshot
        .scan(&Scan::prefix(DROP_PREFIX.to_vec()))
        .try_fold(0, |n, item| item.map(|_| n + 1))
        .context("the keys under the prefix read")?;
    let entry = snapshot.get(&initial).context("readable")?.is_some();
    let kept = snapshot.get(b"\x00keep").context("readable")?.is_some();
    Ok((under, entry, kept))
}

/// The rounds the writer's `log` mode acknowledged: the whole lines of its log beside the store, `<dir>.rounds`, each
/// synced before its round's batch was written; 0 when it wrote none, so that the log does not exist.
///
/// # Errors
///
/// A [`TestError`] when the log exists but does not read, or holds a line that is not the next round's number.
pub fn acknowledged_rounds(dir: &Path) -> TestResult<u64> {
    let text = match std::fs::read_to_string(dir.with_extension("rounds")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).context("the round log reads"),
    };
    // A line cut by the kill has no newline after it, and was never acknowledged.
    let whole = text.rsplit_once('\n').map_or("", |(lines, _)| lines);
    let mut rounds = 0u64;
    for line in whole.lines() {
        let round: u64 = line.parse().context("a round's number")?;
        rounds += 1;
        if round != rounds {
            return Err(TestError::new(format!(
                "round {round} logged as round {rounds}"
            )));
        }
    }
    Ok(rounds)
}

/// Writes round `round`'s batch as the writer's `batches` and `log` modes write it, unsynced: [`KEYS_PER_BATCH`] keys
/// at version `round`, recording `round` as applied. Recovery replays a round so.
///
/// # Errors
///
/// A [`TestError`] when the store refuses the batch.
pub fn replay_round(store: &FjallStore, round: u64) -> TestResult {
    let mut batch = WriteBatch::new();
    let keys = u32::try_from(KEYS_PER_BATCH).context("a batch's key count fits a key's 4 bytes")?;
    (0..keys).for_each(|j| {
        let mut key = b"\x00b".to_vec();
        key.extend_from_slice(&round.to_be_bytes());
        key.extend_from_slice(&j.to_be_bytes());
        batch.push(Entry::put(key, round, 0x04, vec![7; 100]));
    });
    batch.set_applied_ts(round);
    store
        .apply(batch, PersistMode::Buffer)
        .context("the replayed round is written")
}
