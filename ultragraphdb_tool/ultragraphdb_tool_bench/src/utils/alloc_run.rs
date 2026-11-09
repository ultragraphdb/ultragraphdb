/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `alloc_bench`'s run: the heap allocations between a DirectWrite's frame and its record (task 12.5b), counted by the
//! global allocator the binary installs. Each frame takes the path a partition runs it on, through the transaction
//! layer: the frame checked and viewed in place (`FrameView`, `DirectWriteView`), its terms validated and its changes
//! read from it (`FrameTerms`, `FrameChanges`), a read-write transaction begun at the watermark on the buffers the
//! pipeline lends and the changes written, resolving each term to its ID, then its commit building the record. The
//! counts are taken at each step's end, so each frame's allocations are split into those three steps; the pipeline's
//! commit step, the log's sync and fjall's apply follow outside the counted span, so the buffers come back as a
//! partition's do. The partition host's routing and session table around those calls are not run.

use std::path::Path;

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, DirectWriteView, FrameView, Header, MAX_FRAME_SIZE,
};
use ultragraphdb_store_kv::Durability;
use ultragraphdb_transactions_commit::{Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};
use ultragraphdb_transactions_txn::{
    Commit, FrameChanges, FrameTerms, Mode, Prepared, Transaction, TxnBuffers, TxnLimits,
};

use crate::errors::bench_error::BenchError;
use crate::traits::alloc_count::AllocCount;
use crate::traits::clock::Clock;
use crate::types::alloc_args::AllocArgs;
use crate::types::client_shape::ClientShape;
use crate::types::summary::Summary;
use crate::types::wall_clock::WallClock;
use crate::utils::setup::{BenchApplier, open_applier};

/// The cluster the frames and the log's rounds name.
const CLUSTER: u64 = 1;
/// The stream each frame names, as the frontend fills it.
const STREAM: u64 = 1;
/// The store's posting cache, as the other runs give it.
const CACHE_MB: u64 = 128;

/// What one frame cost: its terms, the allocations of each step (its frame read, its write, its commit), its
/// reallocations and the bytes its allocations asked for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FrameCost {
    terms: u64,
    read: u64,
    write: u64,
    commit: u64,
    reallocations: u64,
    bytes: u64,
}

/// Runs `alloc_bench`'s `args`, reading the process's allocations from `count`, and returns its report, one JSON line.
///
/// # Errors
///
/// [`BenchError::usage`] when the run's directory exists; the store's, the log's, a frame's, a transaction's or the
/// pipeline's error, which ends the run.
pub fn run_allocs(args: &AllocArgs, count: &impl AllocCount) -> Result<String, BenchError> {
    if args.dir().exists() {
        return Err(BenchError::usage(format!(
            "{} exists; every run takes a fresh directory",
            args.dir().display()
        )));
    }
    std::fs::create_dir_all(args.dir())?;
    let a = open_applier(&args.dir().join("store"), Durability::Log, CACHE_MB)?;
    let runtime = compio::runtime::Runtime::new()?;
    let costs = runtime.block_on(frames(&a, &args.dir().join("wal"), args, count))?;
    Ok(report(args, &costs))
}

/// Turns `args.frames()` frames into records, counting each, and commits each record through the pipeline.
///
/// Cancellation: driven to its end by [`run_allocs`]; dropped, the log keeps the rounds synced so far.
async fn frames(
    a: &BenchApplier,
    wal_dir: &Path,
    args: &AllocArgs,
    count: &impl AllocCount,
) -> Result<Vec<FrameCost>, BenchError> {
    let config = WalConfig::new(CLUSTER, false);
    let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
    let (wal, _) = Wal::open(CompioStorage, wal_dir, config)
        .await
        .map_err(|e| BenchError::commit(e.to_string()))?;
    let mut pipeline =
        Pipeline::new(a, wal, bounds).map_err(|e| BenchError::commit(e.to_string()))?;
    let clock = WallClock::new();
    let mut shape = ClientShape::new("alloc", 0, args.relations(), (1, args.hubs()), 1)?;
    // At most `frames` costs, each pushed once.
    let mut costs = Vec::with_capacity(usize::try_from(args.frames()).unwrap_or(0));
    for ticket in 0..args.frames() {
        let frame = shape
            .next_direct()?
            .frame(Header::new(CLUSTER, Command::DirectWrite).with_stream(STREAM))
            .map_err(|e| BenchError::commit(e.to_string()))?;
        let buffers = pipeline.buffers_mut().pop().unwrap_or_default();
        let (prepared, cost) = record_of(a, &frame, buffers, count, micros(&clock))?;
        costs.push(cost);
        pipeline
            .commit(ticket, prepared, micros(&clock))
            .map_err(failed)?;
        pipeline.sync(micros(&clock)).await.map_err(failed)?;
        let refused = pipeline
            .replies_mut()
            .drain(..)
            .filter(|reply| reply.outcome().is_err())
            .count();
        if refused > 0 {
            return Err(BenchError::commit(format!("frame {ticket} was refused")));
        }
    }
    Ok(costs)
}

/// The record of `frame`, on `buffers`, at `now`, and what each step allocated.
fn record_of(
    a: &BenchApplier,
    frame: &[u8],
    buffers: TxnBuffers,
    count: &impl AllocCount,
    now: u64,
) -> Result<(Prepared, FrameCost), BenchError> {
    let start = count.counted();
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE).map_err(failed)?;
    let one = DirectWriteView::from_frame(&view).map_err(failed)?;
    let terms = FrameTerms::of(one.terms()).map_err(failed)?;
    let changes = FrameChanges::of_records(one.tenant(), one.terms(), &terms, one.len(), |index| {
        one.change(index)
    })
    .map_err(failed)?;
    let read = count.counted();
    let mut txn = Transaction::begin(
        a,
        one.tenant(),
        Mode::ReadWrite,
        0,
        now,
        TxnLimits::default(),
        buffers,
    )
    .map_err(failed)?;
    txn.write(&changes, now).map_err(failed)?;
    let written = count.counted();
    let Commit::ReadWrite(prepared) = txn.commit(0, 0, &[], now).map_err(failed)? else {
        return Err(BenchError::commit(
            "a read-write transaction ended read-only".to_string(),
        ));
    };
    let end = count.counted();
    let cost = FrameCost {
        terms: one.terms().len() as u64,
        read: read.0 - start.0,
        write: written.0 - read.0,
        commit: end.0 - written.0,
        reallocations: end.1 - start.1,
        bytes: end.2 - start.2,
    };
    Ok((prepared, cost))
}

/// The time on `clock` in whole microseconds, as the pipeline takes it.
fn micros(clock: &WallClock) -> u64 {
    u64::try_from(clock.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// A step's failure as the run's.
fn failed(error: impl std::fmt::Display) -> BenchError {
    BenchError::commit(error.to_string())
}

/// The report of a run: the frames, their terms, and the allocations of each step and in all, per frame and per term, as
/// the median, the least, the 99th percentile and the most of the frames.
fn report(args: &AllocArgs, costs: &[FrameCost]) -> String {
    let of = |figure: &dyn Fn(&FrameCost) -> u64| {
        let mut values: Vec<u64> = costs.iter().map(figure).collect();
        let least = values.iter().copied().min().unwrap_or(0);
        (Summary::of(&mut values), least)
    };
    let json = |(summary, least): (Summary, u64)| {
        format!(
            "{{\"p50\":{},\"min\":{least},\"p99\":{},\"max\":{}}}",
            summary.p50(),
            summary.p99(),
            summary.max()
        )
    };
    let total = |cost: &FrameCost| cost.read + cost.write + cost.commit;
    // Per term in thousandths, so the summary keeps three decimals of a ratio.
    let per_term = |cost: &FrameCost| total(cost) * 1_000 / cost.terms.max(1);
    let (ratio, _) = of(&per_term);
    format!(
        concat!(
            "{{\"run\":\"allocs\",\"frames\":{},\"relations_per_frame\":{},\"terms_per_frame\":{},",
            "\"allocations_per_frame\":{{\"read\":{},\"write\":{},\"commit\":{},\"total\":{}}},",
            "\"reallocations_per_frame\":{},\"bytes_per_frame\":{},\"allocations_per_term_p50\":{:.3}}}"
        ),
        costs.len(),
        args.relations(),
        json(of(&|cost| cost.terms)),
        json(of(&|cost| cost.read)),
        json(of(&|cost| cost.write)),
        json(of(&|cost| cost.commit)),
        json(of(&total)),
        json(of(&|cost| cost.reallocations)),
        json(of(&|cost| cost.bytes)),
        ratio.p50() as f64 / 1_000.0,
    )
}
