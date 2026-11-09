/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The probe of an inbound point read (P24's accepted cost, "about 20× the store probes of one reverse list at
//! in-degree 20"): for each in-degree `d`, an object `<o{d}>` with `d` subjects pointing at it through one predicate,
//! each subject homed by its own bucket and written in a commit of its own by its partition, through the partitions'
//! hosts and logs, registering at the object's home; then the reverse read of each object at the last closed epoch,
//! fanned out over the partitions its referrer set names, beside the same read on one partition holding every commit.
//! Each read's store probes are counted at the stores: the posting lists it reads, which the cache serves or the store
//! does, and its point reads, the referrer set's.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use oxrdf::{GraphName, NamedNode};
use ultragraphdb_cluster_partition::{Fanout, ReverseQuery};
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_core_ids::{Epoch, SystemId};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_kv::Durability;
use ultragraphdb_store_quads::iri_bucket;

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::commit_shape::CommitShape;
use crate::types::inbound_args::InboundArgs;
use crate::types::read_counts::ReadCounts;
use crate::types::summary::Summary;
use crate::types::wall_clock::WallClock;
use crate::utils::partitions_run::{
    BenchPartition, Commits, Plan, Stats, TENANT, open_parts, owner, write_all,
};
use crate::utils::setup::{BenchApplier, open_applier, summary_json};

/// The predicate every probed edge takes.
const PREDICATE: &str = "inbound";
/// Each partition's offered rate, in commits a second: the probe measures reads, not the writes before them.
const RATE: u64 = 2_000;

/// What reads counted at the stores: the posting lists read, those the cache served, those read from the store, and
/// the point reads of a key's newest version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Probes {
    list_reads: u64,
    cache_hits: u64,
    store_reads: u64,
    point_reads: u64,
}

impl Probes {
    /// What `counts` counted so far.
    fn of(counts: &ReadCounts) -> Self {
        let (lookups, hits, store_reads, _) = counts.get();
        Self {
            list_reads: lookups,
            cache_hits: hits,
            store_reads,
            point_reads: counts.point_reads(),
        }
    }

    /// The counts of `self` and `other` together.
    fn plus(self, other: Self) -> Self {
        Self {
            list_reads: self.list_reads.saturating_add(other.list_reads),
            cache_hits: self.cache_hits.saturating_add(other.cache_hits),
            store_reads: self.store_reads.saturating_add(other.store_reads),
            point_reads: self.point_reads.saturating_add(other.point_reads),
        }
    }

    /// What was counted since `before`, an earlier count of the same stores.
    fn since(self, before: Self) -> Self {
        Self {
            list_reads: self.list_reads.saturating_sub(before.list_reads),
            cache_hits: self.cache_hits.saturating_sub(before.cache_hits),
            store_reads: self.store_reads.saturating_sub(before.store_reads),
            point_reads: self.point_reads.saturating_sub(before.point_reads),
        }
    }
}

/// One side of a probe: what a read found, how many it asked, and the counts and the time of the timed reads.
struct Measured {
    found: usize,
    asked: usize,
    probes: Probes,
    nanos: Summary,
}

/// Runs the probe of `args` and returns its report, one JSON line.
///
/// # Errors
///
/// [`BenchError::usage`] when the run's directory exists; the stores', the logs' and the partitions' errors;
/// [`BenchError::commit`] when a read finds another number of subjects than its object's in-degree.
pub fn run_inbound(args: &InboundArgs) -> Result<String, BenchError> {
    if args.dir().exists() {
        return Err(BenchError::usage(format!(
            "{} exists; every run takes a fresh directory",
            args.dir().display()
        )));
    }
    std::fs::create_dir_all(args.dir())?;
    let (placement, parts) = open_parts(args.dir(), args.partitions(), args.cache_mb())?;
    let plans = listed_plans(args.degrees(), &placement, parts.len())?;
    let (stats, last_close, _) = write_all(args.dir(), &parts, plans)?;
    let one = open_applier(&args.dir().join("one"), Durability::Log, args.cache_mb())?;
    for kept in stats.iter().flat_map(|s| &s.kept) {
        let _applied = one.apply(kept)?;
    }
    let rows = args
        .degrees()
        .iter()
        .map(|&degree| probe(args.reads(), &parts, &one, degree, last_close))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(report(args, &stats, last_close, &rows))
}

/// The commits of each partition, by position: for each in-degree `d`, one commit a subject `<i{d}-{k}>`, `k` below
/// `d`, written by the partition homing it, of its edge to `<o{d}>`.
fn listed_plans(
    degrees: &[usize],
    placement: &PlacementMap,
    partitions: usize,
) -> Result<Vec<Plan>, BenchError> {
    let mut lists: Vec<VecDeque<Changeset>> = vec![VecDeque::new(); partitions];
    let predicate = CommitShape::iri(PREDICATE);
    for &degree in degrees {
        let object = CommitShape::iri(&format!("o{degree}"));
        for k in 0..degree {
            let subject = CommitShape::iri(&format!("i{degree}-{k}"));
            let bucket = iri_bucket(TENANT, subject.as_str()).ok_or_else(|| no_home(&subject))?;
            // Partitions are numbered from 1, so partition `n` writes the list at `n - 1`.
            let at = usize::try_from(placement.partition_of(bucket, Epoch::ZERO).get())
                .map_err(|_| no_home(&subject))?;
            let list = at
                .checked_sub(1)
                .and_then(|at| lists.get_mut(at))
                .ok_or_else(|| no_home(&subject))?;
            list.push_back(Changeset::new(TENANT).with(Change::Insert {
                graph: GraphName::DefaultGraph,
                subject: subject.into(),
                predicate: predicate.clone(),
                object: object.clone().into(),
            }));
        }
    }
    Ok(lists
        .into_iter()
        .map(|list| Plan {
            commits: Commits::Listed(list),
            rate: RATE,
            end: Duration::MAX,
            count: 0,
            keep: true,
        })
        .collect())
}

/// The probe of `<o{degree}>`: its reverse read over the partitions, then on one partition, each read once and then
/// `reads` times timed.
fn probe(
    reads: u64,
    parts: &[Arc<BenchPartition>],
    one: &BenchApplier,
    degree: usize,
    epoch: Epoch,
) -> Result<(usize, Measured, Measured), BenchError> {
    let object = CommitShape::iri(&format!("o{degree}"));
    let predicate = CommitShape::iri(PREDICATE);
    let home = owner(parts, &object).ok_or_else(|| no_home(&object))?;
    let query = ReverseQuery::new(
        home.id(),
        TENANT,
        Graph::Default,
        id_in(
            owner(parts, &predicate)
                .ok_or_else(|| no_home(&predicate))?
                .applier(),
            &predicate,
        )?,
        id_in(home.applier(), &object)?,
        epoch,
    );
    let fanout = Fanout::over(parts.iter().map(Arc::as_ref).collect());
    let mut subjects: Vec<SystemId> = Vec::with_capacity(degree);
    let spread = measure(
        reads,
        || parts_probes(parts),
        || {
            let asked = fanout
                .subjects(&query, &mut subjects)
                .map_err(|e| BenchError::commit(e.to_string()))?;
            Ok((subjects.len(), asked.len()))
        },
    )?;
    let (p, o) = (id_in(one, &predicate)?, id_in(one, &object)?);
    let counts = one.quads().lists().store().reads();
    let single = measure(
        reads,
        || Probes::of(counts),
        || {
            let found =
                one.quads()
                    .subjects(TENANT, Graph::Default, p, &EncodedTerm::id(o), u64::MAX)?;
            Ok((found.len(), 1))
        },
    )?;
    for found in [spread.found, single.found] {
        if found != degree {
            return Err(BenchError::commit(format!(
                "the reverse read of {object} found {found} subjects, not its {degree}"
            )));
        }
    }
    Ok((degree, spread, single))
}

/// `read` once, then `reads` times timed, its probes counted by `counts` over the timed reads.
fn measure<C, F>(reads: u64, counts: C, mut read: F) -> Result<Measured, BenchError>
where
    C: Fn() -> Probes,
    F: FnMut() -> Result<(usize, usize), BenchError>,
{
    let (found, asked) = read()?;
    let before = counts();
    let mut nanos = Vec::with_capacity(usize::try_from(reads).unwrap_or(0));
    for _ in 0..reads {
        let clock = WallClock::new();
        let _found = read()?;
        nanos.push(u64::try_from(clock.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    Ok(Measured {
        found,
        asked,
        probes: counts().since(before),
        nanos: Summary::of(&mut nanos),
    })
}

/// The probes every partition's store counted so far.
fn parts_probes(parts: &[Arc<BenchPartition>]) -> Probes {
    parts.iter().fold(Probes::default(), |sum, part| {
        sum.plus(Probes::of(part.applier().quads().lists().store().reads()))
    })
}

/// The ID `applier`'s dictionary holds for `iri`.
fn id_in(applier: &BenchApplier, iri: &NamedNode) -> Result<SystemId, BenchError> {
    applier
        .quads()
        .id_of(TENANT, iri.as_str())?
        .ok_or_else(|| BenchError::commit(format!("{iri} has no ID")))
}

fn no_home(iri: &NamedNode) -> BenchError {
    BenchError::commit(format!("{iri} has no partition"))
}

/// The run's report: the writes' counts, and for each in-degree the partitions asked and each side's probes per read and
/// time.
fn report(
    args: &InboundArgs,
    stats: &[Stats],
    last_close: Epoch,
    rows: &[(usize, Measured, Measured)],
) -> String {
    let sum = |field: fn(&Stats) -> u64| stats.iter().map(field).sum::<u64>();
    let rows: Vec<String> = rows
        .iter()
        .map(|(degree, spread, single)| {
            format!(
                "{{\"in_degree\":{degree},\"partitions_asked\":{},\"fanout\":{},\"one_partition\":{}}}",
                spread.asked,
                side_json(spread, args.reads()),
                side_json(single, args.reads()),
            )
        })
        .collect();
    format!(
        concat!(
            "{{\"run\":\"inbound\",\"partitions\":{},\"cache_mb\":{},\"reads\":{},\"last_close\":{},",
            "\"commits\":{},\"refused\":{},\"retried\":{},\"degrees\":[{}]}}"
        ),
        args.partitions(),
        args.cache_mb(),
        args.reads(),
        last_close.get(),
        sum(|s| s.committed),
        sum(|s| s.refused),
        sum(|s| s.retried),
        rows.join(","),
    )
}

/// One side of a probe as JSON: its probes per read, to a hundredth, and its time per read in nanoseconds.
fn side_json(side: &Measured, reads: u64) -> String {
    // Each `as f64` turns a count into a figure of the report: exact below 2^53, which a probe's counts stay under.
    let per = |count: u64| count as f64 / reads as f64;
    format!(
        concat!(
            "{{\"list_reads\":{:.2},\"cache_hits\":{:.2},\"store_reads\":{:.2},\"point_reads\":{:.2},",
            "\"ns\":{}}}"
        ),
        per(side.probes.list_reads),
        per(side.probes.cache_hits),
        per(side.probes.store_reads),
        per(side.probes.point_reads),
        summary_json(side.nanos),
    )
}
