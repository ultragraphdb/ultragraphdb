/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition's walks of its bound transactions timed with many open (task 12.5b): one partition over fjall and its
//! log, as `wal_bench --partitions` opens it, holding `n` read-write transactions bound as Binds open them. Then two walks,
//! each timed `repeats` times: `sweep_bound`, which a partition core runs at the start of every turn to find the
//! transactions past their limit, none of them here; and the walk `bound_answered` makes for each answer to a registration,
//! timed as the host's turn taking one answer naming a write no bound transaction asked, the walk's longest, against the
//! host's turn taking none. Every answer to a one-frame write's registration walks the same way before the host finds its
//! parked write.

use std::sync::mpsc::Sender;
use std::time::Duration;

use ultragraphdb_cluster_partition::{
    PartitionError, PartitionHost, PartitionMailbox, PartitionMessage,
};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::bound_args::BoundArgs;
use crate::types::summary::Summary;
use crate::types::wall_clock::WallClock;
use crate::utils::partitions_run::{TENANT, open_parts};

/// The cluster the partition's log names.
const CLUSTER: u64 = 1;
/// The principal each bound transaction acts for.
const PRINCIPAL: u64 = 1;
/// The connection every bound transaction's key names: a frontend's one role connection carries them all.
const CONNECTION: u64 = 1;
/// The posting cache of the partition's store, in MiB.
const CACHE_MB: u64 = 64;

/// The host of the run's partition, over its log.
type BenchHost<'a> = PartitionHost<
    'a,
    Wal<CompioStorage>,
    crate::types::timed_store::TimedStore,
    crate::types::counting_cache::CountingCache<
        ultragraphdb_store_kv::MokaListCache<ultragraphdb_store_posting::CachedList>,
    >,
    ultragraphdb_store_kv::ThreadExecutor,
    ultragraphdb_traits_blocks::TestBlockSource,
>;

/// What one run timed, in nanoseconds each.
struct Walks {
    sweep_ns: Vec<u64>,
    turn_ns: Vec<u64>,
    answered_turn_ns: Vec<u64>,
}

/// Runs the bound-walk measurement of `args` and returns its report, one JSON line.
///
/// # Errors
///
/// [`BenchError::usage`] when the run's directory exists; the store's, the log's and the partition's errors; a sweep that
/// ended a transaction, which no transaction is old enough for.
pub fn run_bound(args: &BoundArgs) -> Result<String, BenchError> {
    if args.dir().exists() {
        return Err(BenchError::usage(format!(
            "{} exists; every run takes a fresh directory",
            args.dir().display()
        )));
    }
    let (_, parts) = open_parts(args.dir(), 1, CACHE_MB)?;
    let part = parts
        .first()
        .ok_or_else(|| BenchError::usage("no partition opened".to_string()))?;
    let runtime = compio::runtime::Runtime::new()?;
    let walks = runtime.block_on(async {
        let config = WalConfig::new(CLUSTER, false);
        let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
        let (log, _) = Wal::open(CompioStorage, &args.dir().join("wal"), config)
            .await
            .map_err(|e| BenchError::commit(e.to_string()))?;
        let first = Epoch::new(1).map_err(|e| BenchError::commit(e.to_string()))?;
        let pipeline = Pipeline::new(part.applier(), log, bounds)
            .map_err(|e| BenchError::commit(e.to_string()))?
            .with_partition(part.id().get(), first, CloseRequest::default(), &[])
            .bound(part.id(), part.placement());
        let mailbox = PartitionMailbox::mesh(&[part.id()])
            .pop()
            .ok_or_else(|| BenchError::usage("no mailbox".to_string()))?;
        let own = mailbox
            .sender(part.id())
            .ok_or_else(|| BenchError::usage("no sender to the partition".to_string()))?;
        let mut host = PartitionHost::new(part, pipeline, mailbox);
        for n in 0..args.bound() {
            let _read_version = host
                .bind(
                    (CONNECTION, n),
                    identity(n),
                    (TENANT, PRINCIPAL, Epoch::ZERO),
                    0,
                )
                .map_err(|e| BenchError::commit(e.to_string()))?;
        }
        time_walks(&mut host, &own, args.repeats()).await
    })?;
    Ok(report(args, walks))
}

/// Times each walk `repeats` times, in turn: the sweep, the host's turn taking nothing, and its turn taking one answer to a
/// registration no bound transaction asked.
async fn time_walks(
    host: &mut BenchHost<'_>,
    own: &Sender<PartitionMessage>,
    repeats: u64,
) -> Result<Walks, BenchError> {
    // `repeats` values each, pushed once per repeat.
    let capacity = usize::try_from(repeats).unwrap_or(0);
    let mut walks = Walks {
        sweep_ns: Vec::with_capacity(capacity),
        turn_ns: Vec::with_capacity(capacity),
        answered_turn_ns: Vec::with_capacity(capacity),
    };
    let clock = WallClock::new();
    // Every transaction was bound at 0, so at 1 µs none is past its limit and the sweep walks them all and ends none.
    let now_us = 1;
    for repeat in 0..repeats {
        let swept = clock.elapsed();
        let ended = host.sweep_bound(now_us);
        walks.sweep_ns.push(nanos(clock.elapsed() - swept));
        if !ended.is_empty() {
            return Err(BenchError::commit(format!(
                "the sweep ended {} transactions",
                ended.len()
            )));
        }
        let idle = clock.elapsed();
        host.turn(now_us)
            .await
            .map_err(|e| BenchError::commit(e.to_string()))?;
        walks.turn_ns.push(nanos(clock.elapsed() - idle));
        own.send(PartitionMessage::Answered {
            write: u64::MAX - repeat,
            index: 0,
            answer: Err(PartitionError::timed_out(1)),
        })
        .map_err(|_| BenchError::commit("the partition's mailbox closed".to_string()))?;
        let answering = clock.elapsed();
        host.turn(now_us)
            .await
            .map_err(|e| BenchError::commit(e.to_string()))?;
        walks
            .answered_turn_ns
            .push(nanos(clock.elapsed() - answering));
    }
    Ok(walks)
}

/// The 16-byte identity of bound transaction `n`, distinct for each.
fn identity(n: u64) -> [u8; 16] {
    let mut identity = [0u8; 16];
    identity[..8].copy_from_slice(&n.to_be_bytes());
    identity
}

/// `elapsed` in whole nanoseconds.
fn nanos(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
}

/// The report of a run: each walk's times in nanoseconds.
fn report(args: &BoundArgs, mut walks: Walks) -> String {
    format!(
        concat!(
            "{{\"run\":\"bound_walks\",\"bound\":{},\"repeats\":{},\"sweep_ns\":{},\"turn_ns\":{},",
            "\"answered_turn_ns\":{}}}"
        ),
        args.bound(),
        args.repeats(),
        summary_json(&Summary::of(&mut walks.sweep_ns)),
        summary_json(&Summary::of(&mut walks.turn_ns)),
        summary_json(&Summary::of(&mut walks.answered_turn_ns)),
    )
}

/// `summary` as a JSON object.
fn summary_json(summary: &Summary) -> String {
    format!(
        "{{\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}}",
        summary.p50(),
        summary.p99(),
        summary.max(),
        summary.mean()
    )
}
