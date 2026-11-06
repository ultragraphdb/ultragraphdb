/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client's writes and reads through the real frontend, compute and storage: where a write lands, and a read of it at the
//! default level.

use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_client_compio::ReadLevel;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, EntriesView, Placement, PlacementView, Scan, ScanAt, ScanRange,
};
use ultragraphdb_store_quads::{iri_bucket, term_key};
use ultragraphdb_tests::{CLUSTER, FRONTEND_BINARY, SECRET, scratch};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{Pattern, TENANT, direct_of, iri, read_whole, write_of};
use crate::cluster::{Cluster, client, shared_client, start_own};

const B: &str = "http://ex.org/b";
const KNOWS: &str = "http://ex.org/knows";

/// What the frontend logs when a partition refused a first request as another's.
const RESENT: &str = "a first request refused as another partition's is resent to its owner";

/// How many read-only transactions a read of a fresh write takes at most, each at a newer snapshot.
const SNAPSHOTS: u32 = 250;

/// The partitions of the routing scenario's cluster.
const PARTITIONS: u32 = 2;

/// How many IRIs the routing scenario tries for one of a partition's buckets, each bucket as likely as another.
const IRIS_TRIED: u32 = 10_000;

/// What one write of the routing scenario did.
#[derive(Debug, Clone)]
pub(crate) struct Routed {
    partition: u32,
    version: u64,
    /// Whether each partition, the first first, holds the write's subject.
    held: Vec<bool>,
}

/// How long a role takes at most to answer a probe of these steps.
const PROBE: Duration = Duration::from_secs(10);

/// `probe`'s answer within [`PROBE`], a failure naming `what` it waited for past it.
async fn bounded<T>(what: &str, probe: impl Future<Output = TestResult<T>>) -> TestResult<T> {
    compio::time::timeout(PROBE, probe)
        .await
        .map_err(|_| TestError::new(format!("{what} did not answer within {PROBE:?}")))?
}

/// The ranges of the frontend's placement map, as the frontend's own Placement answers them: first bucket, past bucket and partition.
async fn placement_ranges(cluster: &Cluster) -> TestResult<Vec<(u32, u32, u32)>> {
    bounded("the frontend's Placement", placed_ranges(cluster)).await
}

async fn placed_ranges(cluster: &Cluster) -> TestResult<Vec<(u32, u32, u32)>> {
    let mut frontend = RoleClient::dial(
        &Address::parse(&cluster.frontend)?,
        CLUSTER,
        "storage",
        SECRET,
    )
    .await?;
    let placed = frontend.request(&Placement::request(0)).await?;
    let view = placed.view()?;
    let map = PlacementView::from_frame(&view)?;
    Ok((0..map.range_count())
        .filter_map(|index| map.range(index))
        .map(|range| (range.start(), range.end(), range.partition()))
        .collect())
}

/// An IRI whose bucket `ranges` give to `partition`.
fn iri_of_partition(ranges: &[(u32, u32, u32)], partition: u32) -> TestResult<String> {
    (0..IRIS_TRIED)
        .map(|at| format!("http://ex.org/routed{at}"))
        .find(|iri| {
            iri_bucket(TENANT, iri).is_some_and(|bucket| {
                ranges.iter().any(|(start, end, holder)| {
                    *holder == partition && *start <= bucket.get() && bucket.get() < *end
                })
            })
        })
        .context("an IRI of the partition")
}

/// Whether partition `partition` at `address` holds `iri` in its dictionary, read as the frontend reads it.
async fn holds_term(address: &str, partition: u32, iri: &str) -> TestResult<bool> {
    let what = format!("partition {partition}'s Scan of its dictionary");
    bounded(&what, scanned_term(address, partition, iri)).await
}

async fn scanned_term(address: &str, partition: u32, iri: &str) -> TestResult<bool> {
    let mut storage =
        RoleClient::dial(&Address::parse(address)?, CLUSTER, "frontend", SECRET).await?;
    let key = term_key(TENANT, iri);
    let past = [key.as_slice(), &[0]].concat();
    let mut scan = Scan::new(ScanAt::Latest, 0, 0, 0, [0; 16], 0, 0);
    scan.push_range(ScanRange::new(partition, &key, &past));
    storage.send_only(&scan).await?;
    let frame = storage.next_frame().await?;
    let view = frame.view()?;
    let entries = EntriesView::from_frame(&view)?;
    Ok(entries.entries().iter().next().is_some())
}

#[when(
    "a client writes one statement per partition, each subject an IRI of that partition's buckets"
)]
fn writes_one_per_partition(w: &mut Steps) -> TestResult {
    let cluster = start_own(w.carrier, PARTITIONS)?;
    let routed = Runtime::new()?.block_on(async {
        let ranges = placement_ranges(&cluster).await?;
        let mut client = client(&cluster.client).await?;
        let mut routed = Vec::new();
        for partition in 1..=PARTITIONS {
            let subject = iri_of_partition(&ranges, partition)?;
            let transaction = client.begin(TENANT, false).await?;
            client
                .write(&transaction, &write_of(&subject, KNOWS, &iri(B))?)
                .await?;
            let committed = client.commit(transaction, None).await?;
            // The frontend logs a first request a partition refused as another's, which storage's correction would hide.
            let log = std::fs::read_to_string(scratch(FRONTEND_BINARY))?;
            if log.contains(RESENT) {
                return Err(TestError::new(format!(
                    "the Write of {subject} went first to a partition not owning its bucket: {log}"
                )));
            }
            let mut held = Vec::new();
            for (at, address) in (1..).zip(&cluster.partitions) {
                held.push(holds_term(address, at, &subject).await?);
            }
            routed.push(Routed {
                partition,
                version: committed.version(),
                held,
            });
        }
        TestResult::Ok(routed)
    })?;
    w.cluster = Some(cluster);
    w.routed = routed;
    Ok(())
}

#[then(
    "the frontend sends each Write first to the partition owning its subject's bucket, and each commit's reply reaches the client"
)]
fn reached_each_owner(w: &mut Steps) {
    assert_eq!(w.routed.len() as u32, PARTITIONS, "{}", w.over());
    for routed in &w.routed {
        assert!(routed.version > 0, "a commit version: {}", w.over());
        let expected: Vec<bool> = (1..=PARTITIONS).map(|at| at == routed.partition).collect();
        assert_eq!(routed.held, expected, "{routed:?} {}", w.over());
    }
    w.cluster = None;
}

#[when(
    "a client writes `<a> <knows> <b>` in one frame, and reads `<a> <knows> ?o` in read-only transactions at the default level"
)]
fn one_frame_then_read(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|name| w.iri(name));
    let address = shared_client(w.carrier)?;
    w.answers = Runtime::new()?.block_on(async {
        let mut client = client(&address).await?;
        let _written = client
            .direct_write(direct_of(&a, &knows, &iri(&b))?, None)
            .await?;
        let mut answers = Vec::new();
        for _ in 0..SNAPSHOTS {
            let reader = client.begin(TENANT, true).await?;
            if reader.level() != ReadLevel::Snapshot {
                return Err(TestError::new(format!(
                    "the default level: {:?}",
                    reader.level()
                )));
            }
            let pattern = Pattern::of(Some(&a), Some(&knows));
            let answer = read_whole(&mut client, &reader, &pattern).await?;
            client.abort(reader).await?;
            let seen = !answer.rows.is_empty();
            answers.push(answer);
            if seen {
                return Ok(answers);
            }
            compio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(TestError::new(format!(
            "no snapshot of {SNAPSHOTS} read the write"
        )))
    })?;
    Ok(())
}

#[then("a read returns `<b>` at the snapshot level, and no read returns anything else")]
fn read_back(w: &mut Steps) -> TestResult {
    let (last, earlier) = w.answers.split_last().context("an answer")?;
    assert_eq!(last.column(0), vec![w.iri("b")], "{}", w.over());
    for answer in earlier {
        assert!(answer.rows.is_empty(), "{answer:?} {}", w.over());
    }
    Ok(())
}
