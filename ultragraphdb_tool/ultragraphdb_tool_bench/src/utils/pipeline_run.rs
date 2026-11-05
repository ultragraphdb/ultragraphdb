/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Task 12.5's measurement: the stage-4 workload through stage 5's commit path, to weigh the write-ahead log's write
//! volume beside fjall's journal for the same commits (the design's D1, "Every round is written twice"). A source
//! thread releases the Dgraph-shaped commits of `store_bench apply`, as changesets of quads, at the offered rate into
//! a channel; the commit thread runs each as a read-write transaction at the visibility watermark, hands its proposal
//! to the [`Pipeline`] over the write-ahead log on [`CompioStorage`] and the changeset applier over fjall, and syncs:
//! each round is closed by the previous sync, appended, synced, then applied to fjall as one batch with
//! `PersistMode::Buffer`. The store is opened with `Durability::Log`, so fjall's journal is persisted manually and the
//! log is the only stream synced per round, as the design has it; the store is synced before and after the run, so
//! the journal's bytes are read whole. No checkpoint runs, so the log keeps every round it wrote and its segments'
//! bytes are its write volume. Rollups do not run: they are fjall's local maintenance, outside the log, and the
//! comparison is of the commits' bytes alone. At most 256 commits are in flight, Dgraph's pending-proposal limit, as in the apply run.

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_kv::Durability;
use ultragraphdb_traits_log::OrderedLog;
use ultragraphdb_transactions_commit::{Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};
use ultragraphdb_transactions_txn::{Commit, Mode, Prepared, Transaction, TxnBuffers, TxnLimits};

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::commit_shape::CommitShape;
use crate::types::dir_sizes::DirSizes;
use crate::types::pacer::Pacer;
use crate::types::pipeline_args::PipelineArgs;
use crate::types::size::Size;
use crate::types::summary::Summary;
use crate::types::wall_clock::WallClock;
use crate::utils::setup::{BenchApplier, open_applier, prepare, size_json};

/// The hubs and relation types of the apply run's commits.
const HUBS: u64 = 1_000;
const TYPES: u64 = 10;
/// The tenant the commits write, the root.
const TENANT: u64 = 0;
/// The cluster the log's rounds name.
const CLUSTER: u64 = 1;
/// Dgraph's limit on mutation proposals in flight, as in the apply run.
const PENDING_PROPOSALS: u64 = 256;

/// What the commit thread counted.
#[derive(Debug, Default)]
struct Counts {
    committed: u64,
    aborted: u64,
    round_us: Vec<u64>,
}

/// The run's files after it: the log's segments, the log's other files (its superblock), and the store's; and the
/// bytes fjall's journals held before the run's first commit and after its last.
#[derive(Debug, Clone, Copy, Default)]
struct Files {
    segments: Size,
    superblock: Size,
    store: DirSizes,
    journal_before: u64,
    journal_after: u64,
}

/// Runs the pipeline measurement of `args` and returns its report, one JSON line.
///
/// # Errors
///
/// [`BenchError::usage`] when the run's directory exists; the store's, the applier's, the log's, a transaction's or
/// the pipeline's error, which ends the run.
pub fn run_pipeline(args: &PipelineArgs) -> Result<String, BenchError> {
    if args.dir().exists() {
        return Err(BenchError::usage(format!(
            "{} exists; every run takes a fresh directory",
            args.dir().display()
        )));
    }
    std::fs::create_dir_all(args.dir())?;
    let store_dir = args.dir().join("store");
    let wal_dir = args.dir().join("wal");
    let a = open_applier(&store_dir, Durability::Log, args.cache_mb())?;
    let shape = CommitShape::new(args.relations(), HUBS, TYPES, 1);
    prepare(&a, &shape, TYPES)?;
    let data_path = a.quads().lists().store().inner().data_path().to_path_buf();
    // The journal the declarations and the hubs wrote, before the run's first commit, persisted to be read.
    a.quads().lists().sync()?;
    let journal_before = journal_written(&store_dir)?;

    let config = WalConfig::new(CLUSTER, args.power_loss_protection());
    let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
    let runtime = compio::runtime::Runtime::new()?;
    let clock = WallClock::new();
    let (counts, rounds, elapsed) = runtime.block_on(commit_loop(
        &a, &wal_dir, config, bounds, args, shape, clock,
    ))?;
    a.quads().lists().sync()?;
    drop(a);
    let files = Files {
        segments: wal_files(&wal_dir, true)?,
        superblock: wal_files(&wal_dir, false)?,
        store: DirSizes::of(&store_dir, &data_path)?,
        journal_before,
        journal_after: journal_written(&store_dir)?,
    };
    Ok(report(args, &counts, rounds, elapsed, &files))
}

/// The source thread and the commit loop; returns the counts, the log's committed position and the run's length.
async fn commit_loop(
    a: &BenchApplier,
    wal_dir: &Path,
    config: WalConfig,
    bounds: RoundBounds,
    args: &PipelineArgs,
    shape: CommitShape,
    clock: WallClock,
) -> Result<(Counts, u64, Duration), BenchError> {
    let (wal, _) = Wal::open(CompioStorage, wal_dir, config)
        .await
        .map_err(|e| BenchError::commit(e.to_string()))?;
    let mut pipeline =
        Pipeline::new(a, wal, bounds).map_err(|e| BenchError::commit(e.to_string()))?;
    // Shared with the source thread, which reads it to hold the pending limit; this loop stores to it.
    let replied = Arc::new(AtomicU64::new(0));
    let rx = spawn_source(args.rate(), args.seconds(), shape, clock, replied.clone());
    let micros = |timer: &WallClock| u64::try_from(timer.elapsed().as_micros()).unwrap_or(u64::MAX);
    let mut counts = Counts::default();
    let mut ticket = 0u64;
    let mut waiting = Vec::new();
    let mut open = true;
    // Ends once the source has closed its channel, at the run's deadline, and nothing is in flight.
    loop {
        // Nothing in flight: wait for the next commit, or end once the source has stopped.
        if !pipeline.needs_sync() {
            if !open {
                break;
            }
            match rx.recv() {
                Ok(changeset) => waiting.push(changeset),
                Err(_) => break,
            }
        }
        open = take_waiting(&rx, &mut waiting) && open;
        for changeset in waiting.drain(..) {
            let now = micros(&clock);
            let buffers = pipeline.buffers_mut().pop().unwrap_or_default();
            let prepared = prepare_commit(a, &changeset, buffers, now)?;
            pipeline
                .commit(ticket, prepared, now)
                .map_err(|e| BenchError::commit(e.to_string()))?;
            ticket += 1;
        }
        if pipeline.needs_sync() {
            let start = clock.elapsed();
            pipeline
                .sync(micros(&clock))
                .await
                .map_err(|e| BenchError::commit(e.to_string()))?;
            counts
                .round_us
                .push(u64::try_from((clock.elapsed() - start).as_micros()).unwrap_or(u64::MAX));
        }
        for reply in pipeline.replies_mut().drain(..) {
            match reply.outcome() {
                Ok(_) => counts.committed += 1,
                Err(_) => counts.aborted += 1,
            }
        }
        replied.store(counts.committed + counts.aborted, Ordering::Release);
    }
    let elapsed = clock.elapsed();
    Ok((counts, pipeline.log().committed().get(), elapsed))
}

/// Moves every commit waiting in `rx` to `waiting`; whether the source is still open.
fn take_waiting(rx: &mpsc::Receiver<Changeset>, waiting: &mut Vec<Changeset>) -> bool {
    // Each turn takes a waiting commit or returns; the source holds at most `PENDING_PROPOSALS` unreplied.
    loop {
        match rx.try_recv() {
            Ok(changeset) => waiting.push(changeset),
            Err(TryRecvError::Empty) => return true,
            Err(TryRecvError::Disconnected) => return false,
        }
    }
}

/// Runs `changeset` as a read-write transaction at the visibility watermark, on `buffers`, and returns its proposal.
fn prepare_commit(
    a: &BenchApplier,
    changeset: &Changeset,
    buffers: TxnBuffers,
    now: u64,
) -> Result<Prepared, BenchError> {
    let mut txn = Transaction::begin(
        a,
        TENANT,
        Mode::ReadWrite,
        0,
        now,
        TxnLimits::default(),
        buffers,
    )
    .map_err(|e| BenchError::commit(e.to_string()))?;
    txn.write(changeset, now)
        .map_err(|e| BenchError::commit(e.to_string()))?;
    let Commit::ReadWrite(prepared) = txn
        .commit(0, 0, &[], now)
        .map_err(|e| BenchError::commit(e.to_string()))?
    else {
        return Err(BenchError::commit(
            "a read-write transaction ended read-only".to_string(),
        ));
    };
    Ok(prepared)
}

/// The source: releases `shape`'s commits at `rate` for `seconds`, at most [`PENDING_PROPOSALS`] beyond those
/// `replied`, then closes the channel.
#[expect(
    clippy::disallowed_methods,
    reason = "a benchmark feeds its pipeline from a thread of its own; nothing in the storage crates runs on it"
)]
fn spawn_source(
    rate: u64,
    seconds: u64,
    mut shape: CommitShape,
    clock: WallClock,
    replied: Arc<AtomicU64>,
) -> mpsc::Receiver<Changeset> {
    let (tx, rx) = mpsc::channel::<Changeset>();
    let _source: std::thread::JoinHandle<()> = std::thread::spawn(move || {
        let mut pacer = Pacer::new(rate);
        let (mut sent, mut owed) = (0u64, 0u64);
        let end = Duration::from_secs(seconds);
        // Both loops end at the run's deadline, `end`, or when the commit loop stops listening.
        'run: while clock.elapsed() < end {
            owed += pacer.take_due(&clock);
            while owed > 0 && clock.elapsed() < end {
                if sent - replied.load(Ordering::Acquire) >= PENDING_PROPOSALS {
                    std::thread::sleep(Duration::from_micros(100));
                    owed += pacer.take_due(&clock);
                    continue;
                }
                if tx.send(shape.next_commit()).is_err() {
                    break 'run;
                }
                sent += 1;
                owed -= 1;
            }
            let next = pacer.next_due();
            let now = clock.elapsed();
            if next > now {
                std::thread::sleep((next - now).min(Duration::from_millis(1)));
            }
        }
    });
    rx
}

/// The sizes of the log's files in `dir`: its segments when `segments`, every other file otherwise.
pub(crate) fn wal_files(dir: &Path, segments: bool) -> std::io::Result<Size> {
    std::fs::read_dir(dir)?.try_fold(Size::default(), |size, entry| {
        let entry = entry?;
        let meta = entry.metadata()?;
        let is_segment = entry.path().extension().is_some_and(|e| e == "segment");
        Ok(match meta.is_file() && is_segment == segments {
            true => size.plus(Size::new(meta.len(), meta.blocks() * 512)),
            false => size,
        })
    })
}

/// The bytes fjall's journals in the store's `dir` hold: each `*.jnl` at the top up to its last byte that is not zero.
/// fjall creates a journal at its full length, 64 MiB, and appends to it, so neither its length nor its allocated
/// blocks are what it was written: the unwritten rest reads as zeros.
fn journal_written(dir: &Path) -> std::io::Result<u64> {
    std::fs::read_dir(dir)?.try_fold(0u64, |sum, entry| {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "jnl") {
            return Ok(sum);
        }
        let bytes = std::fs::read(&path)?;
        let written = bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
        Ok(sum + written as u64)
    })
}

/// The report of a run: what was offered and committed, the rounds, the log's and the store's files, and the bytes
/// each wrote per relation committed, the journal's net of what it held before the run.
fn report(
    args: &PipelineArgs,
    counts: &Counts,
    rounds: u64,
    elapsed: Duration,
    files: &Files,
) -> String {
    let secs = elapsed.as_secs_f64();
    let relations = counts.committed * args.relations() as u64;
    let mut round_us = counts.round_us.clone();
    let round = Summary::of(&mut round_us);
    let wal = files.segments.apparent();
    let journal = files.journal_after.saturating_sub(files.journal_before);
    let per_relation = |bytes: u64| bytes as f64 / relations.max(1) as f64;
    format!(
        concat!(
            "{{\"run\":\"pipeline\",\"offered_commits_per_s\":{},\"relations_per_commit\":{},",
            "\"power_loss_protection\":{},\"seconds\":{:.2},\"commits_offered\":{},\"commits_committed\":{},",
            "\"commits_aborted\":{},\"achieved_commits_per_s\":{:.0},\"achieved_relations_per_s\":{:.0},",
            "\"rounds\":{},\"commits_per_round\":{:.2},",
            "\"round_sync_apply_us\":{{\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}},",
            "\"wal\":{{\"segments\":{},\"superblock\":{}}},",
            "\"store\":{{\"data\":{},\"journal\":{},\"metadata\":{},\"total\":{}}},",
            "\"journal_written\":{{\"before_run\":{},\"after_run\":{}}},",
            "\"wal_bytes_per_relation\":{:.1},\"journal_bytes_per_relation\":{:.1},\"wal_to_journal\":{:.2}}}"
        ),
        args.rate(),
        args.relations(),
        args.power_loss_protection(),
        secs,
        counts.committed + counts.aborted,
        counts.committed,
        counts.aborted,
        counts.committed as f64 / secs,
        relations as f64 / secs,
        rounds,
        counts.committed as f64 / rounds.max(1) as f64,
        round.p50(),
        round.p99(),
        round.max(),
        round.mean(),
        size_json(files.segments),
        size_json(files.superblock),
        size_json(files.store.data()),
        size_json(files.store.journal()),
        size_json(files.store.metadata()),
        size_json(files.store.total()),
        files.journal_before,
        files.journal_after,
        per_relation(wal),
        per_relation(journal),
        wal as f64 / journal.max(1) as f64,
    )
}

/// The report's arithmetic over figures made by hand: it is private, and a run's figures depend on the machine.
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{Counts, Files, report};
    use crate::types::dir_sizes::DirSizes;
    use crate::types::pipeline_args::PipelineArgs;
    use crate::types::size::Size;

    #[test]
    fn test_a_run_of_no_commits_divides_by_nothing() {
        let args = PipelineArgs::new(PathBuf::from("d"), 1, 1, 1, 0, true);
        let files = Files {
            journal_before: 5,
            journal_after: 3,
            ..Files::default()
        };
        let line = report(&args, &Counts::default(), 0, Duration::from_secs(1), &files);
        // No commits, rounds or relations, and a journal smaller than before: each ratio over them is 0, not NaN.
        for field in [
            "\"power_loss_protection\":true",
            "\"commits_offered\":0,\"commits_committed\":0,\"commits_aborted\":0",
            "\"rounds\":0,\"commits_per_round\":0.00",
            "\"round_sync_apply_us\":{\"p50\":0,\"p99\":0,\"max\":0,\"mean\":0.0}",
            "\"wal_bytes_per_relation\":0.0,\"journal_bytes_per_relation\":0.0,\"wal_to_journal\":0.00}",
        ] {
            assert!(line.contains(field), "{field} in {line}");
        }
        assert!(!line.contains("NaN") && !line.contains("inf"), "{line}");
    }

    #[test]
    fn test_the_report_states_each_figure_of_the_run() {
        let args = PipelineArgs::new(PathBuf::from("d"), 100, 2, 10, 128, false);
        let counts = Counts {
            committed: 8,
            aborted: 1,
            round_us: vec![300, 100, 200],
        };
        let files = Files {
            segments: Size::new(16_384, 16_384),
            superblock: Size::new(8_192, 8_192),
            store: DirSizes::default(),
            journal_before: 1_000,
            journal_after: 9_192,
        };
        let line = report(&args, &counts, 3, Duration::from_secs(2), &files);
        // By hand. 9 replies, 8 committed, 16 relations, over 2 s: 4 commits and 8 relations a second; 8 commits in 3
        // rounds, 2.67 a round. Round times 100, 200, 300 us: nearest ranks 200 and 300, mean 200. The segments'
        // 16,384 bytes over 16 relations, 1,024 each; the journal's 9,192 - 1,000 = 8,192 bytes, 512 each; the ratio
        // 16,384 / 8,192.
        let expected = concat!(
            "{\"run\":\"pipeline\",\"offered_commits_per_s\":100,\"relations_per_commit\":2,",
            "\"power_loss_protection\":false,\"seconds\":2.00,\"commits_offered\":9,\"commits_committed\":8,",
            "\"commits_aborted\":1,\"achieved_commits_per_s\":4,\"achieved_relations_per_s\":8,",
            "\"rounds\":3,\"commits_per_round\":2.67,",
            "\"round_sync_apply_us\":{\"p50\":200,\"p99\":300,\"max\":300,\"mean\":200.0},",
            "\"wal\":{\"segments\":{\"du_bytes\":16384,\"apparent_bytes\":16384},",
            "\"superblock\":{\"du_bytes\":8192,\"apparent_bytes\":8192}},",
            "\"store\":{\"data\":{\"du_bytes\":0,\"apparent_bytes\":0},\"journal\":{\"du_bytes\":0,\"apparent_bytes\":0},",
            "\"metadata\":{\"du_bytes\":0,\"apparent_bytes\":0},\"total\":{\"du_bytes\":0,\"apparent_bytes\":0}},",
            "\"journal_written\":{\"before_run\":1000,\"after_run\":9192},",
            "\"wal_bytes_per_relation\":1024.0,\"journal_bytes_per_relation\":512.0,\"wal_to_journal\":2.00}"
        );
        assert_eq!(line, expected);
    }
}
