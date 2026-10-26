/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Task 10.2's first measurement: the apply thread at a fixed offered rate. A source thread releases Dgraph-shaped
//! commits, as changesets of quads, at the rate into a channel; the apply thread takes everything waiting in one
//! receive, as Dgraph's apply loop takes one Raft `Ready`'s committed entries (bounded at 64 MiB,
//! `MaxCommittedSizePerReady`), and applies them as one batch of the changeset applier: each a request, all
//! committed in one store batch. The run reports the lists the commits read per relation, the posting cache's lookups
//! and hits, a commit's updates of the entries it wrote among them, and the store's reads of a list and those that
//! found nothing, a rollup's among them. At most 256
//! commits are in flight, Dgraph's pending-proposal limit; past it the source waits, and the run measures the rate the
//! apply thread reaches. Rollups run beside it on a thread of their own, as Dgraph's rollup goroutine does, unless the
//! run turns them off.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ultragraphdb_core_mapping::{Change, Changeset};
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
    lists_read: u64,
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
    let lists = a.quads().lists();
    let data_path = lists.store().inner().data_path().to_path_buf();
    let _ = lists.store().take_calls();
    let reads_start = lists.store().reads().get();
    // The applier's own write calls at open and at the declarations are not the run's rollups.
    let rollup_calls_start = lists.store().other_calls();

    let clock = WallClock::new();
    let stop = Arc::new(AtomicBool::new(false));
    let applied = Arc::new(AtomicU64::new(0));
    let cpu_start = process_cpu_secs();

    let rollups = args
        .rollups()
        .then(|| spawn_rollups(&a, clock, stop.clone()));
    let (tx, rx) = mpsc::channel::<Changeset>();
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
    let rollup_calls = {
        let (calls, nanos) = lists.store().other_calls();
        (calls - rollup_calls_start.0, nanos - rollup_calls_start.1)
    };
    let reads = {
        let (l, h, s, e) = lists.store().reads().get();
        let (l0, h0, s0, e0) = reads_start;
        (l - l0, h - h0, s - s0, e - e0)
    };
    lists.sync()?;
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
        reads,
        rss,
        rss_peak,
        sizes,
    ))
}

/// Applies `commits` as one apply step: one batch of the changeset applier. Returns the batch's figures and how many
/// commits failed.
fn apply_batch(
    a: &Arc<BenchApplier>,
    clock: &WallClock,
    commits: &[Changeset],
) -> Result<(BatchStat, u64), BenchError> {
    let start = clock.elapsed();
    let results = a.apply_batch(commits)?;
    let nanos = u64::try_from((clock.elapsed() - start).as_nanos()).unwrap_or(u64::MAX);
    let failed = results.iter().filter(|r| r.is_err()).count() as u64;
    let lists_read = results
        .iter()
        .flatten()
        .map(|applied| applied.lists_read() as u64)
        .sum();
    let write_calls = a.quads().lists().store().take_calls();
    let write_nanos = write_calls.iter().map(WriteCall::nanos).sum();
    Ok((
        BatchStat {
            commits: commits.len() as u64,
            lists_read,
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
            match a.quads().lists().run_rollups(clock.elapsed().as_secs()) {
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

/// The size of a commit as a Raft entry, estimated: each statement's terms as text.
fn encoded_size(c: &Changeset) -> usize {
    c.changes()
        .iter()
        .map(|change| match change {
            Change::Insert {
                subject,
                predicate,
                object,
                ..
            } => subject.to_string().len() + predicate.as_str().len() + object.to_string().len(),
            _ => 24,
        })
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
    reads: (u64, u64, u64, u64),
    rss: Option<u64>,
    rss_peak: Option<u64>,
    sizes: DirSizes,
) -> String {
    let secs = elapsed.as_secs_f64();
    let applied: u64 = stats.iter().map(|s| s.commits).sum();
    let relations = applied * args.relations() as u64;
    let lists_read: u64 = stats.iter().map(|s| s.lists_read).sum();
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
            "\"achieved_commits_per_s\":{:.0},\"achieved_relations_per_s\":{:.0},\"lists_read_per_relation\":{:.2},",
            "\"cache_lookups\":{},\"cache_hits\":{},\"cache_hit_share\":{:.3},\"store_list_reads\":{},\"store_reads_found_nothing\":{},",
            "\"source_waits_on_pending_limit\":{},",
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
        lists_read as f64 / relations.max(1) as f64,
        reads.0,
        reads.1,
        reads.1 as f64 / reads.0.max(1) as f64,
        reads.2,
        reads.3,
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

/// The report's arithmetic and the size estimate, over figures made by hand: both are private, and a run's figures
/// depend on the machine.
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use oxrdf::{GraphName, Literal, NamedNode, Term};
    use ultragraphdb_core_mapping::{Change, Changeset};
    use ultragraphdb_store_kv::Durability;

    use super::{BatchStat, encoded_size, report};
    use crate::types::apply_args::ApplyArgs;
    use crate::types::dir_sizes::DirSizes;
    use crate::types::write_call::WriteCall;

    fn batch(commits: u64, lists_read: u64, nanos: u64, write_calls: Vec<WriteCall>) -> BatchStat {
        BatchStat {
            commits,
            lists_read,
            nanos,
            write_nanos: write_calls.iter().map(WriteCall::nanos).sum(),
            write_calls,
        }
    }

    #[test]
    fn test_the_report_states_each_figure_of_the_run() {
        let args = ApplyArgs::new(
            PathBuf::from("d"),
            100,
            2,
            10,
            Durability::Process,
            true,
            128,
            Some(4),
        );
        let stats = [
            // Two unpressured calls: 1,000,000 ns over 10 entries and 600,000 over 3, 100,000 and 200,000 ns each.
            batch(
                2,
                4,
                3_000_000,
                vec![
                    WriteCall::new(1_000_000, 10, true, 0, 0),
                    WriteCall::new(600_000, 3, true, 19, 3),
                ],
            ),
            // Pressured by 20 level-0 tables: 3,000,000 ns over 10 entries.
            batch(
                4,
                8,
                5_000_000,
                vec![WriteCall::new(3_000_000, 10, true, 20, 0)],
            ),
            // Pressured by 4 sealed memtables, faster than the baseline; and a call of no entries.
            batch(
                2,
                0,
                2_000_000,
                vec![
                    WriteCall::new(300_000, 5, true, 0, 4),
                    WriteCall::new(0, 0, true, 0, 0),
                ],
            ),
        ];
        let line = report(
            &args,
            &stats,
            1,
            9,
            3,
            Duration::from_secs(2),
            Some(3.0),
            Some(0.5),
            None,
            Some(0.25),
            5,
            (6, 1_500_000_000),
            (40, 10, 7, 2),
            Some(1_024),
            None,
            DirSizes::default(),
        );
        // By hand. 8 commits applied, 16 relations, 12 lists read, over 2 s. Batch commits 2, 4, 2: nearest ranks
        // 2 and 4, mean 8 / 3. Batch times 3,000, 5,000, 2,000 us: ranks 3,000 and 5,000, mean 10,000 / 3. Write
        // calls 1,600, 3,000, 300 us: ranks 1,600 and 3,000, mean 4,900 / 3. Write share 4.9 / 10 ms; busy share
        // 10 ms / 2 s. Baseline: the median of 100,000 and 200,000 ns an entry, the lower by nearest rank. Excess:
        // 3,000,000 - 10 * 100,000 = 2,000,000 ns, and 300,000 - 5 * 100,000 below zero, so none; 2.0 ms. Hit share
        // 10 / 40. CPU 3 s over 2 s is 1.5 cores.
        let expected = concat!(
            "{\"run\":\"apply\",\"offered_commits_per_s\":100,\"relations_per_commit\":2,\"durability\":\"Process\",",
            "\"rollups\":true,\"max_batch\":4,\"seconds\":2.00,\"commits_offered\":9,\"commits_applied\":8,\"commit_errors\":1,",
            "\"achieved_commits_per_s\":4,\"achieved_relations_per_s\":8,\"lists_read_per_relation\":0.75,",
            "\"cache_lookups\":40,\"cache_hits\":10,\"cache_hit_share\":0.250,\"store_list_reads\":7,\"store_reads_found_nothing\":2,",
            "\"source_waits_on_pending_limit\":3,",
            "\"batches\":3,\"batch_commits\":{\"p50\":2,\"p99\":4,\"max\":4,\"mean\":2.7},",
            "\"batch_apply_us\":{\"p50\":3000,\"p99\":5000,\"max\":5000,\"mean\":3333.3},",
            "\"batch_write_call_us\":{\"p50\":1600,\"p99\":3000,\"max\":3000,\"mean\":1633.3},",
            "\"write_call_share\":0.490,\"apply_busy_share\":0.005,",
            "\"write_baseline_ns_per_entry\":100000,\"pressured_write_calls\":2,\"stall_excess_ms\":2.0,",
            "\"max_l0_tables\":20,\"max_sealed_memtables\":4,\"rollup_keys\":5,\"rollup_write_calls\":6,\"rollup_write_call_s\":1.50,",
            "\"cpu_s\":3.00,\"cpu_cores\":1.50,\"apply_thread_cpu_s\":0.50,\"source_thread_cpu_s\":null,",
            "\"rollup_thread_cpu_s\":0.25,\"rss_kib\":1024,\"rss_peak_kib\":null,",
            "\"files\":{\"data\":{\"du_bytes\":0,\"apparent_bytes\":0},\"journal\":{\"du_bytes\":0,\"apparent_bytes\":0},",
            "\"metadata\":{\"du_bytes\":0,\"apparent_bytes\":0},\"total\":{\"du_bytes\":0,\"apparent_bytes\":0}},",
            "\"du_bytes_per_relation\":0.0}"
        );
        assert_eq!(line, expected);
    }

    #[test]
    fn test_a_run_of_no_batches_divides_by_nothing() {
        let args = ApplyArgs::new(
            PathBuf::from("d"),
            0,
            1,
            1,
            Durability::Filesystem,
            false,
            0,
            None,
        );
        let line = report(
            &args,
            &[],
            0,
            0,
            0,
            Duration::from_secs(1),
            None,
            None,
            None,
            None,
            0,
            (0, 0),
            (0, 0, 0, 0),
            None,
            None,
            DirSizes::default(),
        );
        // No relations, lookups or batch time: each ratio over them is 0, not NaN.
        for field in [
            "\"durability\":\"Filesystem\"",
            "\"rollups\":false",
            "\"max_batch\":null",
            "\"lists_read_per_relation\":0.00",
            "\"cache_hit_share\":0.000",
            "\"batches\":0,\"batch_commits\":{\"p50\":0,\"p99\":0,\"max\":0,\"mean\":0.0}",
            "\"write_call_share\":0.000,\"apply_busy_share\":0.000",
            "\"write_baseline_ns_per_entry\":0,\"pressured_write_calls\":0,\"stall_excess_ms\":0.0",
            "\"max_l0_tables\":0,\"max_sealed_memtables\":0",
            "\"cpu_s\":null,\"cpu_cores\":null",
            "\"du_bytes_per_relation\":0.0}",
        ] {
            assert!(line.contains(field), "{field} in {line}");
        }
        assert!(!line.contains("NaN") && !line.contains("inf"), "{line}");
    }

    #[test]
    fn test_a_commits_size_is_its_inserts_terms_as_text() {
        let iri = |name: &str| {
            NamedNode::new_unchecked(format!("http://bench.ultragraphdb.example/{name}"))
        };
        let changeset = Changeset::new(0)
            .with(Change::Insert {
                graph: GraphName::DefaultGraph,
                subject: iri("r1").into(),
                predicate: iri("rel_t0_org"),
                object: iri("h3").into(),
            })
            .with(Change::Insert {
                graph: GraphName::DefaultGraph,
                subject: iri("r1").into(),
                predicate: iri("rel_t0_w"),
                object: Term::from(Literal::new_simple_literal("x")),
            })
            .with(Change::DeletePattern {
                graph: GraphName::DefaultGraph,
                subject: iri("r1").into(),
                predicate: iri("rel_t0_w"),
            });
        // Counted by hand: `<…/r1>` is 38 characters, `…/rel_t0_org` 44, `<…/h3>` 38; `…/rel_t0_w` 42, `"x"` 3;
        // any other change 24. 120 + 83 + 24.
        assert_eq!(encoded_size(&changeset), 227);
        assert_eq!(encoded_size(&Changeset::new(0)), 0);
    }
}
