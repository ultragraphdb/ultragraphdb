/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Task 10.2's first measurement: the apply thread at a fixed offered rate. A source thread releases Dgraph-shaped
//! commits at the rate into a channel; the apply thread takes everything waiting in one receive, as Dgraph's apply
//! loop takes one Raft `Ready`'s committed entries (bounded at 64 MiB, `MaxCommittedSizePerReady`), applies each
//! commit as a request reading at the applied timestamp, and commits the batch as one store batch. At most 256
//! commits are in flight, Dgraph's pending-proposal limit; past it the source waits, and the run measures the rate the
//! apply thread reaches. Rollups run beside it on a thread of their own, as Dgraph's rollup goroutine does, unless the
//! run turns them off.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ultragraphdb_proto_pb::pb::Mutations;
use ultragraphdb_traits_store::Executor;

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::apply_args::ApplyArgs;
use crate::types::commit_shape::CommitShape;
use crate::types::dir_sizes::DirSizes;
use crate::types::pacer::Pacer;
use crate::types::summary::Summary;
use crate::types::wall_clock::WallClock;
use crate::types::write_call::WriteCall;
use crate::utils::proc_stats::{process_cpu_secs, rss_kib, thread_cpu_secs};
use crate::utils::setup::{BenchApplier, open_applier, opt_json, prepare, size_json};

const HUBS: u64 = 1_000;
const TYPES: u64 = 10;
/// Dgraph's `MaxCommittedSizePerReady`.
const READY_BYTES: usize = 64 << 20;
/// Dgraph's limit on mutation proposals in flight, `--raft pending-proposals=256` (`worker/proposal.go:41-99`,
/// `worker/server_state.go:34`): the source waits while this many commits are released and not yet applied, as a
/// Dgraph client waits on the limiter, so an apply step never holds more.
const PENDING_PROPOSALS: u64 = 256;

/// What one apply-step batch took.
struct BatchStat {
    commits: u64,
    nanos: u64,
    write_nanos: u64,
    write_calls: Vec<WriteCall>,
}

/// Runs the apply measurement of `args` and returns its report, one JSON line.
///
/// # Errors
///
/// The store's or the applier's error, which ends the run.
pub fn run_apply(args: &ApplyArgs) -> Result<String, BenchError> {
    let a = open_applier(args.dir(), args.durability(), args.cache_mb())?;
    let mut shape = CommitShape::new(args.relations(), HUBS, TYPES, 1);
    prepare(&a, &shape, TYPES)?;
    let data_path = a.lists().store().inner().data_path().to_path_buf();
    let _ = a.lists().store().take_calls();

    let clock = WallClock::new();
    let stop = Arc::new(AtomicBool::new(false));
    let applied = Arc::new(AtomicU64::new(0));
    let cpu_start = process_cpu_secs();

    let rollups = args
        .rollups()
        .then(|| spawn_rollups(&a, clock, stop.clone()));
    let (tx, rx) = mpsc::channel::<Mutations>();
    let (source_done, source_result) = mpsc::channel::<(u64, u64, Option<f64>)>();
    {
        let (rate, seconds, applied) = (args.rate(), args.seconds(), applied.clone());
        a.executor().spawn(move || {
            let mut pacer = Pacer::new(rate);
            let (mut waits, mut sent, mut owed) = (0u64, 0u64, 0u64);
            let end = Duration::from_secs(seconds);
            let cpu = thread_cpu_secs();
            'run: while clock.elapsed() < end {
                owed += pacer.take_due(&clock);
                // Commits due while the limit held them back go out as soon as it allows, one at a time.
                while owed > 0 && clock.elapsed() < end {
                    if sent - applied.load(Ordering::Acquire) >= PENDING_PROPOSALS {
                        waits += 1;
                        std::thread::sleep(Duration::from_micros(100));
                        owed += pacer.take_due(&clock);
                        continue;
                    }
                    if tx.send(shape.next(0)).is_err() {
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
            let cpu = thread_cpu_secs().zip(cpu).map(|(b, a)| b - a);
            let _ = source_done.send((sent, waits, cpu));
        });
    }

    let apply_cpu = thread_cpu_secs();
    let mut stats = Vec::new();
    let mut errors = 0u64;
    while let Ok(first) = rx.recv() {
        let mut commits = vec![first];
        let mut bytes = encoded_size(&commits[0]);
        let cap = args.max_batch().unwrap_or(usize::MAX);
        while bytes < READY_BYTES && commits.len() < cap {
            match rx.try_recv() {
                Ok(m) => {
                    bytes += encoded_size(&m);
                    commits.push(m);
                }
                Err(_) => break,
            }
        }
        let (stat, failed) = apply_batch(&a, &clock, &commits)?;
        errors += failed;
        applied.fetch_add(stat.commits, Ordering::Release);
        stats.push(stat);
    }
    let elapsed = clock.elapsed();
    let apply_cpu = thread_cpu_secs().zip(apply_cpu).map(|(b, a)| b - a);
    stop.store(true, Ordering::Release);
    let (released, waits, source_cpu) = source_result
        .recv()
        .map_err(|e| BenchError::Io(e.to_string()))?;
    let (rolled, rollup_cpu) = match rollups {
        Some(done) => done.recv().map_err(|e| BenchError::Io(e.to_string()))??,
        None => (0, None),
    };
    let cpu = process_cpu_secs().zip(cpu_start).map(|(b, a)| b - a);
    let (rss, rss_peak) = rss_kib();
    let rollup_calls = a.lists().store().other_calls();
    a.lists().sync()?;
    drop(a);
    let sizes = DirSizes::of(args.dir(), &data_path)?;
    Ok(report(
        args,
        &stats,
        errors,
        released,
        waits,
        elapsed,
        cpu,
        apply_cpu,
        source_cpu,
        rollup_cpu,
        rolled,
        rollup_calls,
        rss,
        rss_peak,
        sizes,
    ))
}

/// Applies `commits` as one apply step: each a request at the applied timestamp, all committed in one batch.
/// Returns the batch's figures and how many commits failed.
fn apply_batch(
    a: &Arc<BenchApplier>,
    clock: &WallClock,
    commits: &[Mutations],
) -> Result<(BatchStat, u64), BenchError> {
    let start = clock.elapsed();
    let read_ts = a.lists().applied_ts();
    let mut batch = a.batch();
    for m in commits {
        let mut request = batch.request(read_ts);
        request.apply_edges(m)?;
        batch.push(request);
    }
    let results = batch.commit()?;
    let nanos = u64::try_from((clock.elapsed() - start).as_nanos()).unwrap_or(u64::MAX);
    let failed = results.iter().filter(|r| r.is_err()).count() as u64;
    let write_calls = a.lists().store().take_calls();
    let write_nanos = write_calls.iter().map(WriteCall::nanos).sum();
    Ok((
        BatchStat {
            commits: commits.len() as u64,
            nanos,
            write_nanos,
            write_calls,
        },
        failed,
    ))
}

/// The rollup worker beside the apply thread: one turn of Dgraph's rollup loop, then 1 ms, until `stop`. Sends the
/// keys it rolled up and its CPU time.
#[allow(clippy::type_complexity)]
fn spawn_rollups(
    a: &Arc<BenchApplier>,
    clock: WallClock,
    stop: Arc<AtomicBool>,
) -> mpsc::Receiver<Result<(usize, Option<f64>), BenchError>> {
    let (done, result) = mpsc::channel();
    let a = Arc::clone(a);
    let executor = *a.executor();
    executor.spawn(move || {
        let cpu = thread_cpu_secs();
        let mut rolled = 0usize;
        let outcome = loop {
            if stop.load(Ordering::Acquire) {
                break Ok(());
            }
            match a.lists().run_rollups(clock.elapsed().as_secs()) {
                Ok(0) => std::thread::sleep(Duration::from_millis(1)),
                Ok(n) => rolled += n,
                Err(e) => break Err(BenchError::from(e)),
            }
        };
        let cpu = thread_cpu_secs().zip(cpu).map(|(b, a)| b - a);
        let _ = done.send(outcome.map(|()| (rolled, cpu)));
    });
    result
}

/// The size of a commit as a Raft entry, estimated: each edge's predicate, value and fixed fields.
fn encoded_size(m: &Mutations) -> usize {
    m.edges
        .iter()
        .map(|e| e.attr.len() + e.value.len() + 24)
        .sum()
}

#[allow(clippy::too_many_arguments)]
fn report(
    args: &ApplyArgs,
    stats: &[BatchStat],
    errors: u64,
    released: u64,
    waits: u64,
    elapsed: Duration,
    cpu: Option<f64>,
    apply_cpu: Option<f64>,
    source_cpu: Option<f64>,
    rollup_cpu: Option<f64>,
    rolled: usize,
    rollup_calls: (u64, u64),
    rss: Option<u64>,
    rss_peak: Option<u64>,
    sizes: DirSizes,
) -> String {
    let secs = elapsed.as_secs_f64();
    let applied: u64 = stats.iter().map(|s| s.commits).sum();
    let relations = applied * args.relations() as u64;
    let mut sizes_of: Vec<u64> = stats.iter().map(|s| s.commits).collect();
    let batch_size = Summary::of(&mut sizes_of);
    let mut us: Vec<u64> = stats.iter().map(|s| s.nanos / 1_000).collect();
    let latency = Summary::of(&mut us);
    let mut write_us: Vec<u64> = stats.iter().map(|s| s.write_nanos / 1_000).collect();
    let write = Summary::of(&mut write_us);
    let calls: Vec<&WriteCall> = stats.iter().flat_map(|s| s.write_calls.iter()).collect();
    let batch_nanos: u64 = stats.iter().map(|s| s.nanos).sum();
    let write_nanos: u64 = stats.iter().map(|s| s.write_nanos).sum();
    // The unstalled baseline: the median time per entry of the commit write calls under no stall pressure.
    let mut per_entry: Vec<u64> = calls
        .iter()
        .filter(|c| !c.under_pressure() && c.entries() > 0)
        .map(|c| c.nanos() / c.entries() as u64)
        .collect();
    let baseline = Summary::of(&mut per_entry).p50();
    let pressured: Vec<&&WriteCall> = calls.iter().filter(|c| c.under_pressure()).collect();
    let excess_nanos: u64 = pressured
        .iter()
        .map(|c| c.nanos().saturating_sub(baseline * c.entries() as u64))
        .sum();
    let max_l0 = calls.iter().map(|c| c.l0_tables()).max().unwrap_or(0);
    let max_sealed = calls
        .iter()
        .map(|c| c.sealed_memtables())
        .max()
        .unwrap_or(0);
    let total = sizes.total().allocated();
    format!(
        concat!(
            "{{\"run\":\"apply\",\"offered_commits_per_s\":{},\"relations_per_commit\":{},\"durability\":\"{:?}\",",
            "\"rollups\":{},\"max_batch\":{},\"seconds\":{:.2},\"commits_offered\":{},\"commits_applied\":{},\"commit_errors\":{},",
            "\"achieved_commits_per_s\":{:.0},\"achieved_relations_per_s\":{:.0},\"source_waits_on_pending_limit\":{},",
            "\"batches\":{},\"batch_commits\":{{\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}},",
            "\"batch_apply_us\":{{\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}},",
            "\"batch_write_call_us\":{{\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}},",
            "\"write_call_share\":{:.3},\"apply_busy_share\":{:.3},",
            "\"write_baseline_ns_per_entry\":{},\"pressured_write_calls\":{},\"stall_excess_ms\":{:.1},",
            "\"max_l0_tables\":{},\"max_sealed_memtables\":{},\"rollup_keys\":{},\"rollup_write_calls\":{},\"rollup_write_call_s\":{:.2},",
            "\"cpu_s\":{},\"cpu_cores\":{},\"apply_thread_cpu_s\":{},\"source_thread_cpu_s\":{},",
            "\"rollup_thread_cpu_s\":{},\"rss_kib\":{},\"rss_peak_kib\":{},",
            "\"files\":{{\"data\":{},\"journal\":{},\"metadata\":{},\"total\":{}}},\"du_bytes_per_relation\":{:.1}}}"
        ),
        args.rate(),
        args.relations(),
        args.durability(),
        args.rollups(),
        opt_json(args.max_batch()),
        secs,
        released,
        applied,
        errors,
        applied as f64 / secs,
        relations as f64 / secs,
        waits,
        stats.len(),
        batch_size.p50(),
        batch_size.p99(),
        batch_size.max(),
        batch_size.mean(),
        latency.p50(),
        latency.p99(),
        latency.max(),
        latency.mean(),
        write.p50(),
        write.p99(),
        write.max(),
        write.mean(),
        write_nanos as f64 / batch_nanos.max(1) as f64,
        batch_nanos as f64 / 1e9 / secs,
        baseline,
        pressured.len(),
        excess_nanos as f64 / 1e6,
        max_l0,
        max_sealed,
        rolled,
        rollup_calls.0,
        rollup_calls.1 as f64 / 1e9,
        opt_json(cpu.map(|c| format!("{c:.2}"))),
        opt_json(cpu.map(|c| format!("{:.2}", c / secs))),
        opt_json(apply_cpu.map(|c| format!("{c:.2}"))),
        opt_json(source_cpu.map(|c| format!("{c:.2}"))),
        opt_json(rollup_cpu.map(|c| format!("{c:.2}"))),
        opt_json(rss),
        opt_json(rss_peak),
        size_json(sizes.data()),
        size_json(sizes.journal()),
        size_json(sizes.metadata()),
        size_json(sizes.total()),
        total as f64 / relations.max(1) as f64,
    )
}
