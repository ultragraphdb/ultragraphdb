/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Task 10.2's second measurement: rollups and the compaction filter over one large predicate. It loads
//! `rel_t0_org`, an edge, with commits of `relations` records each linked to one of `hubs` hubs, until each hub's
//! reverse list holds about `versions` deltas, rollups off; then rolls up every hub's reverse list, sets the discard
//! timestamp above every rollup, and runs `Store::compact`, a flush and a major compaction with the GC filter. It
//! counts the predicate's versions above and below the discard timestamp before and after, and the store's files at
//! each step.

use std::sync::Arc;
use std::time::Duration;

use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::commit_shape::CommitShape;
use crate::types::dir_sizes::DirSizes;
use crate::types::rollup_args::RollupArgs;
use crate::types::wall_clock::WallClock;
use crate::utils::proc_stats::{process_cpu_secs, rss_kib};
use crate::utils::setup::{BenchApplier, open_applier, opt_json, prepare, size_json};

/// The commits of one loading batch: few, since a batch's cost grows with the square of its requests (each request
/// clones the batch's stacked layers, `Batch::request`).
const LOAD_BATCH: usize = 16;

/// Versions of a prefix, split at the discard timestamp.
#[derive(Debug, Clone, Copy, Default)]
struct Versions {
    keys: u64,
    above: u64,
    at_or_below: u64,
}

/// Runs the rollup measurement of `args` and returns its report, one JSON line.
///
/// # Errors
///
/// The store's or the applier's error, which ends the run.
pub fn run_rollup(args: &RollupArgs) -> Result<String, BenchError> {
    let a = open_applier(args.dir(), ultragraphdb_store_kv::Durability::Process, 128)?;
    let mut shape = CommitShape::new(args.relations(), args.hubs(), 1, 1);
    prepare(&a, &shape, 1)?;
    let lists = a.quads().lists();
    let data_path = lists.store().inner().data_path().to_path_buf();
    let uid_of = |iri: &str| {
        a.quads()
            .uid_of(0, iri)
            .map_err(ultragraphdb_store_apply::ChangesetError::from)?
            .ok_or_else(|| BenchError::Usage(format!("{iri} has no UID")))
    };
    let predicate = uid_of(CommitShape::iri("rel_t0_org").as_str())?;
    let prefix = |keyspace| key_prefix(keyspace, false, 0, Some(DEFAULT_GRAPH), Some(predicate));
    let (reverse, forward) = (prefix(Keyspace::Reverse), prefix(Keyspace::Forward));

    let clock = WallClock::new();
    let commits = (args.hubs() * args.versions()).div_ceil(args.relations() as u64);
    let load = load(&a, &mut shape, commits, &clock)?;
    lists.sync()?;
    let loaded = DirSizes::of(args.dir(), &data_path)?;

    let start = clock.elapsed();
    let cpu = process_cpu_secs();
    let mut rolled = 0u64;
    for hub in 0..args.hubs() {
        let key = QuadKey::Reverse {
            namespace: 0,
            graph: DEFAULT_GRAPH,
            predicate,
            object: EncodedTerm::uid(uid_of(CommitShape::hub(hub).as_str())?),
        }
        .to_bytes();
        if lists.rollup_key(&key)? {
            rolled += 1;
        }
    }
    let rollup = clock.elapsed() - start;
    let rollup_cpu = process_cpu_secs().zip(cpu).map(|(b, a)| b - a);
    lists.sync()?;
    let rolled_sizes = DirSizes::of(args.dir(), &data_path)?;

    let discard = lists.latest_ts();
    lists.set_discard_ts(discard)?;
    let store = lists.store();
    let before = (
        count(store, &reverse, discard)?,
        count(store, &forward, discard)?,
    );
    let start = clock.elapsed();
    let cpu = process_cpu_secs();
    store.compact()?;
    let compaction = clock.elapsed() - start;
    let compaction_cpu = process_cpu_secs().zip(cpu).map(|(b, a)| b - a);
    let after = (
        count(store, &reverse, discard)?,
        count(store, &forward, discard)?,
    );
    let compacted = DirSizes::of(args.dir(), &data_path)?;
    let (rss, rss_peak) = rss_kib();
    let _ = store.take_calls();
    drop(a);
    let closed = DirSizes::of(args.dir(), &data_path)?;

    let relations = commits * args.relations() as u64;
    Ok(format!(
        concat!(
            "{{\"run\":\"rollup\",\"hubs\":{},\"versions_target\":{},\"relations_per_commit\":{},",
            "\"load\":{{\"commits\":{},\"relations\":{},\"seconds\":{:.2},\"commits_per_s\":{:.0}}},",
            "\"rollup\":{{\"keys\":{},\"seconds\":{:.2},\"keys_per_s\":{:.0},\"deltas_per_s\":{:.0},\"cpu_s\":{}}},",
            "\"discard_ts\":{},",
            "\"reverse_before\":{},\"reverse_after\":{},\"forward_before\":{},\"forward_after\":{},",
            "\"compaction\":{{\"seconds\":{:.2},\"cpu_s\":{},\"reverse_collected\":{},\"forward_collected\":{}}},",
            "\"files_loaded\":{},\"files_rolled_up\":{},\"files_compacted\":{},\"files_closed\":{},",
            "\"du_bytes_per_relation_closed\":{:.1},\"rss_kib\":{},\"rss_peak_kib\":{}}}"
        ),
        args.hubs(),
        args.versions(),
        args.relations(),
        commits,
        relations,
        load.as_secs_f64(),
        commits as f64 / load.as_secs_f64(),
        rolled,
        rollup.as_secs_f64(),
        rolled as f64 / rollup.as_secs_f64(),
        before.0.at_or_below.saturating_sub(before.0.keys) as f64 / rollup.as_secs_f64(),
        opt_json(rollup_cpu.map(|c| format!("{c:.2}"))),
        discard,
        versions_json(before.0),
        versions_json(after.0),
        versions_json(before.1),
        versions_json(after.1),
        compaction.as_secs_f64(),
        opt_json(compaction_cpu.map(|c| format!("{c:.2}"))),
        before.0.above + before.0.at_or_below - after.0.above - after.0.at_or_below,
        before.1.above + before.1.at_or_below - after.1.above - after.1.at_or_below,
        files_json(loaded),
        files_json(rolled_sizes),
        files_json(compacted),
        files_json(closed),
        closed.total().allocated() as f64 / relations.max(1) as f64,
        opt_json(rss),
        opt_json(rss_peak),
    ))
}

/// Applies `commits` commits of `shape`, `LOAD_BATCH` to a batch, as fast as the apply step goes; its time.
fn load(
    a: &Arc<BenchApplier>,
    shape: &mut CommitShape,
    commits: u64,
    clock: &WallClock,
) -> Result<Duration, BenchError> {
    let start = clock.elapsed();
    let mut left = commits;
    while left > 0 {
        let n = left.min(LOAD_BATCH as u64);
        let changesets: Vec<_> = (0..n).map(|_| shape.next_commit()).collect();
        if let Some(Err(e)) = a.apply_batch(&changesets)?.into_iter().find(Result::is_err) {
            return Err(e.into());
        }
        left -= n;
        // The rollups this load queues are not run; the measurement runs them itself.
        let _ = a.quads().lists().store().take_calls();
    }
    Ok(clock.elapsed() - start)
}

/// The keys and versions under `prefix`, every version read, split at `discard`.
fn count<S: Store>(store: &S, prefix: &[u8], discard: u64) -> Result<Versions, BenchError> {
    let snapshot = store.snapshot(u64::MAX)?;
    let mut v = Versions::default();
    let mut last: Option<Vec<u8>> = None;
    for item in snapshot.scan(&Scan::prefix(prefix.to_vec()).all_versions()) {
        let item = item?;
        if last.as_deref() != Some(item.key()) {
            v.keys += 1;
            last = Some(item.key().to_vec());
        }
        if item.version() > discard {
            v.above += 1;
        } else {
            v.at_or_below += 1;
        }
    }
    Ok(v)
}

fn versions_json(v: Versions) -> String {
    format!(
        "{{\"keys\":{},\"versions_above_discard\":{},\"versions_at_or_below_discard\":{}}}",
        v.keys, v.above, v.at_or_below
    )
}

fn files_json(s: DirSizes) -> String {
    format!(
        "{{\"data\":{},\"journal\":{},\"metadata\":{},\"total\":{}}}",
        size_json(s.data()),
        size_json(s.journal()),
        size_json(s.metadata()),
        size_json(s.total())
    )
}
