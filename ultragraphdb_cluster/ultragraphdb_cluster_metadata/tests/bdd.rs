/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata group's scenarios, bound to `features/`, over the write-ahead log on its simulated storage, beside the
//! real sequencer where a scenario closes epochs.

#[path = "utils/cluster.rs"]
mod cluster;
#[path = "utils/harness.rs"]
mod harness;
#[path = "utils/partition_model.rs"]
mod partition_model;

use std::collections::BTreeSet;
use std::fmt;

use cucumber::{World, then, when};
use ultragraphdb_cluster_metadata::{
    LeasedBlocks, MetadataCommand, MetadataConfig, MetadataSnapshot, MetadataState,
};
use ultragraphdb_cluster_placement::{BucketRange, PartitionId, TenantRun};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, Tenant, TypePrefix};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_protocol_scram::{Credentials, MIN_ITERATIONS, ScramClient, ScramServer};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Catalog, CatalogView, Command, FrameView, Header, MAX_FRAME_FLOOR,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_traits_catalog::{CatalogSource, DefaultEntry};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::SimStorage;

use crate::cluster::Cluster;
use crate::harness::{
    Group, block_on, crash_and_recover, epoch, fresh_with, learn_through, node, pid, range,
    recover, sync, tenant, verifier,
};
use crate::partition_model::{AllocatorModel, EpochModel};

/// The cluster the Catalog frames name.
const CATALOG_CLUSTER: u64 = 7;

#[derive(Default, World)]
struct Steps {
    storage: Option<SimStorage>,
    group: Option<Group>,
    cluster: Option<Cluster>,
    config: Option<MetadataConfig>,
    before: Option<(MetadataState, Position)>,
    snapshot: Option<MetadataSnapshot>,
    refusals: Vec<ClusterError>,
    blocks: Vec<SequenceBlock>,
    allocated: Vec<Vec<u64>>,
    allocator_refusal: Option<String>,
    closes: Vec<Close>,
    validated: Vec<(u64, u64)>,
    stamps: Vec<u64>,
    sessions: Vec<u128>,
    evicted: Vec<u128>,
    authenticated: Option<u64>,
    resolved: Vec<Vec<PartitionId>>,
    found: Option<PartitionId>,
    /// A Catalog frame a storage process read.
    catalog_frame: Vec<u8>,
}

impl fmt::Debug for Steps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Steps")
            .field("refusals", &self.refusals)
            .field("blocks", &self.blocks)
            .field("validated", &self.validated)
            .finish_non_exhaustive()
    }
}

impl Steps {
    fn group(&mut self) -> TestResult<&mut Group> {
        if let Some(cluster) = self.cluster.as_mut() {
            return Ok(&mut cluster.group);
        }
        self.group.as_mut().context("the group")
    }

    fn cluster(&mut self) -> TestResult<&mut Cluster> {
        self.cluster.as_mut().context("the cluster")
    }

    fn storage(&self) -> TestResult<SimStorage> {
        self.storage.clone().context("the storage")
    }

    fn config(&self) -> MetadataConfig {
        self.config.unwrap_or_default()
    }

    fn start(&mut self) -> TestResult {
        let (storage, group) = fresh_with(self.config())?;
        self.storage = Some(storage);
        self.group = Some(group);
        Ok(())
    }

    /// Kills the group and recovers it from its log and the snapshot the host stored.
    fn restart(&mut self) -> TestResult {
        let storage = self.storage()?;
        let snapshot = self.snapshot.clone();
        self.group = Some(crash_and_recover(&storage, self.config(), snapshot)?);
        Ok(())
    }

    /// Remembers the durable state and the position it reads at.
    fn remember(&mut self) -> TestResult {
        let group = self.group()?;
        let before = (group.state().clone(), group.applied());
        self.before = Some(before);
        Ok(())
    }
}

/// Offers versions 1 to `through` of `tenant`'s catalog, each holding its own number's text, and syncs them.
fn offer_versions(group: &mut Group, through: u64) -> TestResult {
    for version in 1..=through {
        let _offered =
            group.offer_catalog(tenant(), version - 1, format!("v{version}").as_bytes())?;
    }
    sync(group)
}

/// The placement map's ranges in effect at `at`, of the run of `tenant`.
fn run_ranges(
    group: &Group,
    tenant: Tenant,
    at: u64,
) -> TestResult<Vec<(BucketRange, PartitionId)>> {
    let run = TenantRun::of(tenant).range();
    let map = group.state().placement().context("the placement map")?;
    Ok(map
        .ranges_at(epoch(at)?)
        .into_iter()
        .filter(|(r, _)| r.start() >= run.start() && r.end() <= run.end())
        .collect())
}

/// The latest layout with `moved` taken from whichever ranges cover it and given to `to`, or to no partition.
fn layout_moving(
    group: &Group,
    moved: BucketRange,
    to: Option<PartitionId>,
) -> TestResult<Vec<(BucketRange, PartitionId)>> {
    let map = group.state().placement().context("the placement map")?;
    let latest = map.latest_change();
    let mut layout = Vec::new();
    for (held, partition) in map.ranges_at(latest) {
        if held.end() <= moved.start() || held.start() >= moved.end() {
            layout.push((held, partition));
            continue;
        }
        if held.start() < moved.start() {
            layout.push((range(held.start(), moved.start())?, partition));
        }
        if held.end() > moved.end() {
            layout.push((range(moved.end(), held.end())?, partition));
        }
    }
    if let Some(partition) = to {
        layout.push((moved, partition));
    }
    Ok(layout)
}

// ---------------------------------------------------------------------------------------------------------------------
// The metadata group is a log-backed state machine with one replica
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "the group applies a catalog version, two block leases, a placement change and a principal, and is killed and restarted"
)]
fn applies_and_restarts(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let _first = group.init_placement(3, node())?;
    sync(group)?;
    let _offered = group.offer_catalog(tenant(), 0, b"v1")?;
    let _first_lease = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1_000)?;
    let _second_lease = group.lease_block(TypePrefix::RESOURCE, pid(2)?, 500)?;
    let layout = layout_moving(group, range(0, 64)?, Some(pid(3)?))?;
    let _changed = group.change_placement(&layout)?;
    let _created = group.create_principal("ann", verifier("ann's own password", 1)?, true)?;
    sync(group)?;
    w.remember()?;
    w.restart()
}

#[then("its state after recovery equals its state before the kill, read at the same log position")]
fn state_equal_after_recovery(w: &mut Steps) -> TestResult {
    let (state, position) = w.before.clone().context("the state before the kill")?;
    let group = w.group()?;
    assert_eq!(group.applied(), position);
    assert_eq!(position.get(), 6);
    assert_eq!(group.state(), &state);
    // The commands are all there, not merely two empty states alike.
    assert_eq!(group.state().current_version(tenant()), 1);
    assert_eq!(group.state().next_sequence(TypePrefix::RESOURCE), 1_501);
    assert_eq!(
        group.state().principal("ann").map(|p| p.name()),
        Some("ann")
    );
    // The change is pending as placement version 1, which no close has announced yet.
    assert_eq!(group.state().pending_placement(), 1);
    let map = group.state().placement().context("the map")?;
    assert_eq!(map.latest_change().get(), 0);
    Ok(())
}

#[when("the group holds tenant 0, a snapshot is taken, and the group is killed and restarted")]
fn tenants_over_a_snapshot(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let _created = group.create_tenant(Tenant::new(0))?;
    let _drawn = group.draw_secret([9; 32])?;
    sync(group)?;
    let snapshot = group.snapshot();
    let mut stored = Vec::new();
    snapshot.encode(&mut stored);
    block_on(group.trim(&snapshot))??;
    w.snapshot = Some(MetadataSnapshot::decode(&stored)?);
    w.remember()?;
    w.restart()
}

#[then(
    "the group holds tenant 0 at the same log position, from the snapshot and the commands after it"
)]
fn tenants_recovered(w: &mut Steps) -> TestResult {
    let (state, position) = w.before.clone().context("the state before the kill")?;
    let group = w.group()?;
    assert_eq!(group.applied(), position);
    assert_eq!(group.state(), &state);
    assert_eq!(group.state().tenants(), vec![Tenant::new(0)]);
    assert_eq!(group.state().unknown_secret(), Some(&[9; 32]));
    Ok(())
}

#[when("a snapshot is taken at position 100 and 20 more commands are applied before a restart")]
fn snapshot_at_100(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    for _ in 0..100 {
        let _lease = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 10)?;
    }
    sync(group)?;
    let snapshot = group.snapshot();
    assert_eq!(snapshot.position().get(), 100);
    // The host stores the snapshot's bytes and trims the log through it.
    let mut stored = Vec::new();
    snapshot.encode(&mut stored);
    block_on(group.trim(&snapshot))??;
    for _ in 0..20 {
        let _lease = group.lease_block(TypePrefix::STATEMENT, pid(2)?, 3)?;
    }
    sync(group)?;
    w.snapshot = Some(MetadataSnapshot::decode(&stored)?);
    w.remember()?;
    w.restart()
}

#[then("recovery loads the snapshot and replays exactly the 20 commands after it")]
fn replays_20(w: &mut Steps) -> TestResult {
    let (state, position) = w.before.clone().context("the state before the kill")?;
    let storage = w.storage()?;
    let group = w.group()?;
    assert_eq!(group.replayed(), 20);
    assert_eq!(group.applied(), position);
    assert_eq!(position.get(), 120);
    assert_eq!(group.state(), &state);
    // 100 blocks of 10 from 1, then 20 of 3 from 1.
    assert_eq!(group.state().next_sequence(TypePrefix::RESOURCE), 1_001);
    assert_eq!(group.state().next_sequence(TypePrefix::STATEMENT), 61);
    // The log is trimmed through the snapshot: without it, the rounds before are gone.
    let refusal = recover(&storage, MetadataConfig::new(), None)
        .err()
        .context("a refusal")?;
    assert!(
        refusal.reason().contains("checkpoint 100"),
        "{}",
        refusal.reason()
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Catalog versions take effect at an epoch boundary
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "catalog version 3 is committed as pending while epoch 38 is the latest close, the sequencer closes epoch 39, and rounds of epochs 39 and 40 commit"
)]
fn version_3_at_38(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::new()?;
    offer_versions(&mut cluster.group, 2)?;
    let _closes = cluster.close_through(38)?;
    let _offered = cluster.group.offer_catalog(tenant(), 2, b"v3")?;
    sync(&mut cluster.group)?;
    let close = cluster.close()?;
    let mut partition = EpochModel::at(38);
    w.stamps.push(partition.stamp());
    partition.deliver(&close);
    w.stamps.push(partition.stamp());
    for round in [39, 40] {
        let (version, _) = partition.validate(cluster.group.state(), tenant().get(), round)?;
        w.validated.push((round, version));
    }
    w.closes.push(close);
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "version 3 takes effect at epoch 40, assigned and announced with the close of 39, and the round of epoch 39 is validated against version 2 and the round of epoch 40 against version 3"
)]
fn version_3_at_40(w: &mut Steps) -> TestResult {
    let close = w.closes.first().context("the close of 39")?;
    assert_eq!(close.epoch().get(), 39);
    let announced: Vec<(u64, u64)> = close
        .catalog()
        .iter()
        .map(|v| (v.tenant().get(), v.version()))
        .collect();
    assert_eq!(announced, vec![(tenant().get(), 3)]);
    assert_eq!(close.effective_epoch().map(Epoch::get), Some(40));
    assert_eq!(w.stamps, vec![39, 40]);
    assert_eq!(w.validated, vec![(39, 2), (40, 3)]);
    let state = w.cluster()?.group.state();
    let record = state
        .catalog_at(tenant(), epoch(40)?)?
        .context("the version at 40")?;
    assert_eq!(record.version(), 3);
    assert_eq!(record.effective().map(Epoch::get), Some(40));
    assert_eq!(record.bytes(), b"v3");
    assert!(state.pending_versions().is_empty());
    Ok(())
}

#[when(
    "version 3 of `tenant` is current, a first write of `score` records its default entry, and a client offers version 4 built on version 3"
)]
fn default_then_version_4(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    offer_versions(group, 3)?;
    learn_through(group, 1)?;
    let before = group.state().current_version(tenant());
    let entry = DefaultEntry::new(
        tenant().get(),
        "score".to_string(),
        b"default score".to_vec(),
    );
    let recorded = group.record_default_entry(&entry)?;
    // The record is the round after the three versions and the close.
    assert_eq!(recorded.map(|p| p.get()), Some(5));
    sync(group)?;
    // A prepared statement remembers the version current when it was prepared.
    let after = group.state().current_version(tenant());
    let (version, _) = group.offer_catalog(tenant(), 3, b"v4")?;
    sync(group)?;
    w.stamps = vec![before, after, version];
    Ok(())
}

#[then(
    "the default-entry record of (`tenant`, `score`) is held, version 4 is accepted, and no prepared statement of `tenant` is stale"
)]
fn default_held_version_4(w: &mut Steps) -> TestResult {
    // Current before and after the default entry, and the number the offer took.
    assert_eq!(w.stamps, vec![3, 3, 4]);
    let state = w.group()?.state();
    assert_eq!(
        state.default_entry(tenant(), "score"),
        Some(&b"default score"[..])
    );
    assert_eq!(state.current_version(tenant()), 4);
    let numbers: Vec<u64> = state
        .versions(tenant())
        .iter()
        .map(|v| v.version())
        .collect();
    assert_eq!(numbers, vec![1, 2, 3, 4]);
    // The statement prepared at version 3 reads the version in effect, which only a learned close changes.
    let effective = state
        .catalog_at(tenant(), epoch(2)?)?
        .map(|record| record.version());
    assert_eq!(effective, Some(3));
    Ok(())
}

#[when("a client offers version 4 built on version 2 while version 3 is current")]
fn offers_4_on_2(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    offer_versions(group, 3)?;
    let applied = group.applied();
    let refusal = group
        .offer_catalog(tenant(), 2, b"v4")
        .err()
        .context("a refusal")?;
    assert_eq!(group.applied(), applied);
    w.refusals.push(refusal);
    Ok(())
}

#[then("the offer is refused with an error naming versions 2 and 3, and version 3 stays current")]
fn refused_naming_2_and_3(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?.clone();
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::StaleCatalog {
            tenant: tenant().get(),
            predecessor: 2,
            current: 3
        }
    );
    let text = refusal.to_string();
    assert!(
        text.contains("version 2") && text.contains("version 3"),
        "{text}"
    );
    let group = w.group()?;
    sync(group)?;
    assert_eq!(group.state().current_version(tenant()), 3);
    assert_eq!(group.applied().get(), 3);
    Ok(())
}

#[when(
    "a catalog version of 523,000 bytes is committed, and a storage process at the floor of `max_frame` reads it with Catalog"
)]
fn version_at_its_cap(w: &mut Steps) -> TestResult {
    assert_eq!(MetadataCommand::CATALOG_BYTES_MAX, 523_000);
    w.start()?;
    let group = w.group()?;
    let bytes = vec![7; MetadataCommand::CATALOG_BYTES_MAX];
    let (_version, position) = group.offer_catalog(tenant(), 0, &bytes)?;
    sync(group)?;
    let state = group.state();
    let versions = state.versions(tenant());
    let record = versions.last().context("the version")?;
    assert_eq!(record.bytes(), bytes.as_slice());
    let reply = Catalog::reply(
        tenant().get(),
        record.version(),
        0,
        position.get(),
        record.bytes().to_vec(),
    );
    w.catalog_frame = reply.frame(Header::new(CATALOG_CLUSTER, Command::Catalog))?;
    Ok(())
}

#[then("one Catalog frame holds its whole bytes")]
fn one_catalog_frame(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.catalog_frame, CATALOG_CLUSTER, MAX_FRAME_FLOOR)?;
    let view = CatalogView::from_frame(&frame)?;
    assert_eq!(view.bytes().len(), MetadataCommand::CATALOG_BYTES_MAX);
    assert!(view.bytes().iter().all(|byte| *byte == 7));
    assert!(w.catalog_frame.len() <= MAX_FRAME_FLOOR);
    assert_eq!(MAX_FRAME_FLOOR, 524_288);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// The metadata group leases sequence blocks
// ---------------------------------------------------------------------------------------------------------------------

#[when("partitions 1 and 2 lease blocks of the `Resource` type alternately ten times")]
fn alternate_leases(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let mut blocks = Vec::new();
    for round in 0..10_u64 {
        for partition in [1, 2] {
            // Sizes that differ per lease, so a block's end is not its start plus a constant.
            let size = 100 + round * 10 + u64::from(partition);
            let (block, _) = group.lease_block(TypePrefix::RESOURCE, pid(partition)?, size)?;
            blocks.push(block);
        }
        sync(group)?;
    }
    w.blocks = blocks;
    Ok(())
}

#[then("every block begins at the previous block's end, and no two overlap")]
fn blocks_contiguous(w: &mut Steps) -> TestResult {
    assert_eq!(w.blocks.len(), 20);
    assert_eq!(w.blocks.first().map(|b| b.start().get()), Some(1));
    for pair in w.blocks.windows(2) {
        let [before, after] = pair else {
            return Err(TestError::new("a pair of blocks"));
        };
        assert_eq!(after.start().get(), before.end());
    }
    for (i, a) in w.blocks.iter().enumerate() {
        for b in w.blocks.iter().skip(i + 1) {
            assert!(
                a.end() <= b.start().get() || b.end() <= a.start().get(),
                "{a:?} {b:?}"
            );
        }
    }
    Ok(())
}

#[when("a block 1..1001 is leased and the group is killed and restarted")]
fn lease_then_restart(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let (block, _) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1_000)?;
    assert_eq!((block.start().get(), block.end()), (1, 1_001));
    sync(group)?;
    w.restart()
}

#[then("the next lease of the same type begins at 1001")]
fn next_at_1001(w: &mut Steps) -> TestResult {
    let group = w.group()?;
    let (block, _) = group.lease_block(TypePrefix::RESOURCE, pid(2)?, 7)?;
    assert_eq!((block.start().get(), block.end()), (1_001, 1_008));
    Ok(())
}

#[when("a partition asks for a block larger than `block_size_max`")]
fn oversized_lease(w: &mut Steps) -> TestResult {
    w.config = Some(MetadataConfig::new().with_block_size_max(1_000)?);
    w.start()?;
    let group = w.group()?;
    let refusal = group
        .lease_block(TypePrefix::RESOURCE, pid(1)?, 1_001)
        .err()
        .context("a refusal")?;
    w.refusals.push(refusal);
    Ok(())
}

#[then("the lease is refused with an error naming the size and the maximum")]
fn refused_naming_size(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?.clone();
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::BlockSize {
            size: 1_001,
            max: 1_000
        }
    );
    let text = refusal.to_string();
    assert!(text.contains("1001") && text.contains("1000"), "{text}");
    let group = w.group()?;
    assert_eq!(group.state().next_sequence(TypePrefix::RESOURCE), 1);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// The placement map assigns bucket ranges to partitions
// ---------------------------------------------------------------------------------------------------------------------

/// Starts a group whose map splits every run over three partitions.
fn three_partitions(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let _initialised = group.init_placement(3, node())?;
    sync(group)
}

#[when("the map is initialised for one tenant and three partitions")]
fn initialised_three(w: &mut Steps) -> TestResult {
    three_partitions(w)
}

#[then(
    "that tenant's 65,536 buckets are assigned in three contiguous ranges whose sizes differ by at most one, and every other bucket is assigned to a partition"
)]
fn three_ranges(w: &mut Steps) -> TestResult {
    let group = w.group()?;
    let ranges = run_ranges(group, tenant(), 0)?;
    let sizes: Vec<u32> = ranges.iter().map(|(r, _)| r.buckets()).collect();
    // 65,536 = 3 × 21,845 + 1: the first partition takes the one more.
    assert_eq!(sizes, vec![21_846, 21_845, 21_845]);
    let partitions: Vec<u32> = ranges.iter().map(|(_, p)| p.get()).collect();
    assert_eq!(partitions, vec![1, 2, 3]);
    let map = group.state().placement().context("the map")?;
    let all = map.ranges_at(Epoch::ZERO);
    let mut next = 0;
    for (r, _) in &all {
        assert_eq!(r.start(), next);
        next = r.end();
    }
    assert_eq!(next, BucketRange::END);
    Ok(())
}

#[when("a change moves the range 100..200 from partition 1 to nowhere")]
fn moves_to_nowhere(w: &mut Steps) -> TestResult {
    three_partitions(w)?;
    let group = w.group()?;
    let layout = layout_moving(group, range(100, 200)?, None)?;
    w.remember()?;
    let refusal = w
        .group()?
        .change_placement(&layout)
        .err()
        .context("a refusal")?;
    w.refusals.push(refusal);
    Ok(())
}

#[then("the change is refused with an error naming bucket 100, and the map is unchanged")]
fn refused_naming_100(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?.clone();
    assert_eq!(refusal.0, ClusterErrorEnum::Unassigned { bucket: 100 });
    let (state, position) = w.before.clone().context("the state before")?;
    let group = w.group()?;
    sync(group)?;
    assert_eq!(group.applied(), position);
    assert_eq!(group.state(), &state);
    let map = group.state().placement().context("the map")?;
    assert_eq!(map.latest_change(), Epoch::ZERO);
    Ok(())
}

#[when("partition 2 holds the range 21,846..43,691 and bucket 30,000 is looked up")]
fn looks_up_30000(w: &mut Steps) -> TestResult {
    three_partitions(w)?;
    let group = w.group()?;
    let map = group.state().placement().context("the map")?;
    // Run 0, the buckets 0 to 65,535.
    let run0: Vec<_> = map
        .ranges_at(Epoch::ZERO)
        .into_iter()
        .filter(|(r, _)| r.end() <= TenantRun::BUCKETS)
        .collect();
    assert!(
        run0.contains(&(range(21_846, 43_691)?, pid(2)?)),
        "{run0:?}"
    );
    w.found = Some(map.partition_of(Bucket::new(30_000)?, Epoch::ZERO));
    Ok(())
}

#[then("the lookup answers partition 2")]
fn answers_2(w: &mut Steps) -> TestResult {
    assert_eq!(w.found, Some(pid(2)?));
    Ok(())
}

#[when(
    "a change moving buckets 21,888..21,952 from partition 2 to partition 3 is committed while epoch 19 is the latest close, and the sequencer closes epoch 20"
)]
fn moves_group_at_21(w: &mut Steps) -> TestResult {
    three_partitions(w)?;
    let group = w.group()?;
    let _closed_19 = group.learn_close(epoch(19)?, &[], 0, &[])?;
    let layout = layout_moving(group, range(21_888, 21_952)?, Some(pid(3)?))?;
    let _changed = group.change_placement(&layout)?;
    sync(group)?;
    // The host hands the close of 20 the oldest pending placement version, and the group learns the close.
    let announced = group.state().pending_placement();
    assert_eq!(announced, 1);
    let _closed_20 = group.learn_close(epoch(20)?, &[], announced, &[])?;
    sync(group)?;
    let map = group.state().placement().context("the map")?;
    assert_eq!(map.latest_change(), epoch(21)?);
    // 21,888 / 64 = 342, the group of exactly those 64 buckets.
    let held = BucketGroup::new(342)?;
    let mut resolved = Vec::new();
    for at in [20, 21] {
        let mut partitions = Vec::new();
        map.group_partitions(held, epoch(at)?, &mut partitions);
        resolved.push(partitions);
    }
    w.resolved = resolved;
    Ok(())
}

#[then(
    "the close of epoch 20 assigns it effective epoch 21, and the bucket group holding those 64 buckets resolves to partition 2 at epoch 20 and to partition 3 at epoch 21"
)]
fn resolves_2_then_3(w: &mut Steps) -> TestResult {
    assert_eq!(w.resolved, vec![vec![pid(2)?], vec![pid(3)?]]);
    Ok(())
}

#[when("placement versions 1 and 2 are pending and a close announces version 2")]
fn close_skips_placement_1(w: &mut Steps) -> TestResult {
    three_partitions(w)?;
    let group = w.group()?;
    for to in [pid(1)?, pid(2)?] {
        let layout = layout_moving(group, range(0, 64)?, Some(to))?;
        let _changed = group.change_placement(&layout)?;
    }
    sync(group)?;
    w.remember()?;
    let refusal = w
        .group()?
        .learn_close(epoch(1)?, &[], 2, &[])
        .err()
        .context("a refusal")?;
    w.refusals.push(refusal);
    Ok(())
}

#[then("the group refuses the close, and both versions stay pending")]
fn both_placement_versions_pending(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?.clone();
    assert_eq!(
        refusal,
        ClusterError::contradicts("a close announces the oldest pending placement version")
    );
    let (state, position) = w.before.clone().context("the state before")?;
    let group = w.group()?;
    assert_eq!(group.applied(), position);
    assert_eq!(group.state(), &state);
    assert_eq!(group.state().pending_placement(), 1);
    assert_eq!(group.state().last_learned(), Epoch::ZERO);
    // Version 1 is announced first, then version 2.
    let _first = group.learn_close(epoch(1)?, &[], 1, &[])?;
    let _second = group.learn_close(epoch(2)?, &[], 2, &[])?;
    sync(group)?;
    assert_eq!(group.state().pending_placement(), 0);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Principals live in the metadata group
// ---------------------------------------------------------------------------------------------------------------------

/// The bootstrap secret file's password.
const PASSWORD: &str = "correct horse battery staple";

#[when(
    "a cluster starts for the first time with a bootstrap credential file and a client authenticates with it"
)]
fn bootstrap_and_authenticate(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    assert_eq!(group.state().principals(), 0);
    let file = format!("{PASSWORD}\n");
    let (id, _) = group.bootstrap("admin", file.as_bytes(), &[9; 16], MIN_ITERATIONS)?;
    sync(group)?;
    let mut client = ScramClient::new("admin", PASSWORD, "clientnonce")?;
    let mut server = ScramServer::new("servernonce", [5; 32]).with_iterations(MIN_ITERATIONS);
    let state = group.state();
    let first = client.first_message()?;
    let server_first = server.first(&first, |name| state.lookup(name))?;
    let client_final = client.final_message(&server_first)?;
    let (server_final, principal) = server.last(&client_final)?;
    client.verify(&server_final)?;
    assert_eq!(principal, id);
    w.authenticated = Some(principal);
    Ok(())
}

#[then("authentication succeeds, and the metadata group holds that principal without the password")]
fn holds_without_password(w: &mut Steps) -> TestResult {
    let authenticated = w.authenticated.context("the authenticated principal")?;
    let group = w.group()?;
    let principal = group.state().principal("admin").context("the principal")?;
    assert_eq!(principal.id(), authenticated);
    assert!(principal.administrator());
    let mut bytes = Vec::new();
    group.snapshot().encode(&mut bytes);
    let found = bytes
        .windows(PASSWORD.len())
        .any(|window| window == PASSWORD.as_bytes());
    assert!(!found, "the snapshot holds the password");
    assert!(!format!("{principal:?}").contains(PASSWORD));
    Ok(())
}

#[when("a principal named `ann` exists and another `ann` is created")]
fn duplicate_ann(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let _ann = group.create_principal("ann", verifier("ann's first password", 1)?, false)?;
    let refusal = group
        .create_principal("ann", verifier("ann's second password", 2)?, true)
        .err()
        .context("a refusal")?;
    w.refusals.push(refusal);
    Ok(())
}

#[then("the creation is refused with an error naming `ann`")]
fn refused_naming_ann(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?.clone();
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::PrincipalExists {
            name: "ann".to_string()
        }
    );
    assert!(refusal.to_string().contains("ann"));
    let group = w.group()?;
    sync(group)?;
    assert_eq!(group.state().principals(), 1);
    Ok(())
}

#[when("the only principal with the administrator role is removed")]
fn removes_last_admin(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let _root = group.bootstrap("root", PASSWORD.as_bytes(), &[3; 16], MIN_ITERATIONS)?;
    let _bob = group.create_principal("bob", verifier("bob's own password", 4)?, false)?;
    sync(group)?;
    let refusal = group.remove_principal("root").err().context("a refusal")?;
    w.refusals.push(refusal);
    Ok(())
}

#[then("the removal is refused with an error naming it")]
fn refused_naming_root(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?.clone();
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::LastAdministrator {
            name: "root".to_string()
        }
    );
    assert!(refusal.to_string().contains("root"));
    let group = w.group()?;
    sync(group)?;
    assert_eq!(
        group.state().principal("root").map(|p| p.administrator()),
        Some(true)
    );
    Ok(())
}

#[when("`dave` has two sessions and `carol` one, and `dave` is removed")]
fn removes_dave(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let _root = group.bootstrap("root", PASSWORD.as_bytes(), &[3; 16], MIN_ITERATIONS)?;
    let _dave = group.create_principal("dave", verifier("dave's own password", 7)?, false)?;
    let _carol = group.create_principal("carol", verifier("carol's own password", 6)?, false)?;
    let registered = [(51, "dave"), (52, "carol"), (53, "dave")];
    for (session, principal) in registered {
        let (evicted, _) = group.register_session(session, principal, |_| false)?;
        assert!(evicted.is_empty());
    }
    sync(group)?;
    let (evicted, _) = group.remove_principal("dave")?;
    sync(group)?;
    w.sessions = registered.map(|(session, _)| session).to_vec();
    w.evicted = evicted;
    Ok(())
}

#[then(
    "both of `dave`'s sessions are evicted in the removal and a commit through either is refused as evicted, and `carol`'s session stays"
)]
fn daves_sessions_evicted(w: &mut Steps) -> TestResult {
    let [first, carols, second] = w.sessions[..] else {
        return Err(TestError::new("three sessions"));
    };
    let evicted = w.evicted.clone();
    assert_eq!(
        evicted,
        vec![first, second],
        "dave's sessions, oldest first"
    );
    let state = w.group()?.state();
    assert!(state.principal("dave").is_none(), "dave is removed");
    assert_eq!(state.pending_evictions(), evicted);
    for session in [first, second] {
        // The frontend's owner check refuses the commit before it is routed.
        let refusal = state
            .session_principal(session)
            .err()
            .context("a refusal")?;
        assert_eq!(refusal.0, ClusterErrorEnum::SessionEvicted { session });
    }
    let carol = state.principal("carol").context("carol")?.id();
    assert_eq!(state.session_principal(carols)?, carol);
    assert_eq!(state.sessions(), 1);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// The metadata group registers sessions
// ---------------------------------------------------------------------------------------------------------------------

#[when("a session is registered for `alice` and the group is killed and restarted")]
fn session_then_restart(w: &mut Steps) -> TestResult {
    w.start()?;
    let group = w.group()?;
    let _bob = group.create_principal("bob", verifier("bob's own password", 4)?, false)?;
    let _alice = group.create_principal("alice", verifier("alice's own password", 5)?, false)?;
    let session = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef;
    let (evicted, _) = group.register_session(session, "alice", |_| false)?;
    assert!(evicted.is_empty());
    sync(group)?;
    w.sessions.push(session);
    w.restart()
}

#[then("the session's principal is still `alice`")]
fn still_alice(w: &mut Steps) -> TestResult {
    let session = *w.sessions.first().context("the session")?;
    let state = w.group()?.state();
    let principal = state.session_principal(session)?;
    let name = state
        .principal_by_id(principal)
        .map(|p| p.name().to_string());
    assert_eq!(name.as_deref(), Some("alice"));
    Ok(())
}

#[when("the registry holds `sessions_max` sessions and one more is registered")]
fn one_past_sessions_max(w: &mut Steps) -> TestResult {
    w.config = Some(MetadataConfig::new().with_sessions_max(3)?);
    w.start()?;
    let group = w.group()?;
    let _carol = group.create_principal("carol", verifier("carol's own password", 6)?, false)?;
    for session in [11, 22, 33] {
        let (evicted, _) = group.register_session(session, "carol", |_| false)?;
        assert!(evicted.is_empty());
    }
    let (evicted, _) = group.register_session(44, "carol", |_| false)?;
    sync(group)?;
    w.evicted = evicted;
    Ok(())
}

#[then(
    "the least recently registered session is evicted, and a commit through it is refused as evicted"
)]
fn oldest_evicted(w: &mut Steps) -> TestResult {
    assert_eq!(w.evicted, vec![11]);
    let state = w.group()?.state();
    assert_eq!(state.sessions(), 3);
    assert_eq!(state.pending_evictions(), vec![11]);
    // The frontend's owner check refuses the commit before it is routed.
    let refusal = state.session_principal(11).err().context("a refusal")?;
    assert_eq!(refusal.0, ClusterErrorEnum::SessionEvicted { session: 11 });
    let carol = state.principal("carol").context("carol")?.id();
    for session in [22, 33, 44] {
        assert_eq!(state.session_principal(session)?, carol);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Sequences are unique per type and leased in blocks per partition
// ---------------------------------------------------------------------------------------------------------------------

/// Leases `size` sequences of the `Resource` type for `partition` and supplies the block, once durable, to `blocks`.
fn supply(
    group: &mut Group,
    blocks: &LeasedBlocks,
    partition: u32,
    size: u64,
) -> TestResult<SequenceBlock> {
    let (block, _) = group.lease_block(TypePrefix::RESOURCE, pid(partition)?, size)?;
    sync(group)?;
    blocks.supply(TypePrefix::RESOURCE, block)?;
    Ok(block)
}

#[when(
    "partitions 1 and 2 each lease a block of the `Resource` type and allocate every sequence in it"
)]
fn two_partitions_allocate(w: &mut Steps) -> TestResult {
    w.start()?;
    for (partition, size) in [(1, 500), (2, 300)] {
        let blocks = LeasedBlocks::new();
        let group = w.group()?;
        let block = supply(group, &blocks, partition, size)?;
        let mut allocator = AllocatorModel::new(blocks);
        let mut allocated = Vec::new();
        for _ in 0..block.count() {
            allocated.push(
                allocator
                    .next(TypePrefix::RESOURCE)
                    .map_err(TestError::new)?,
            );
        }
        w.blocks.push(block);
        w.allocated.push(allocated);
    }
    Ok(())
}

#[then("no sequence is allocated twice, and the two blocks do not overlap")]
fn no_collision(w: &mut Steps) -> TestResult {
    let all: Vec<u64> = w.allocated.iter().flatten().copied().collect();
    assert_eq!(all.len(), 800);
    let distinct: BTreeSet<u64> = all.iter().copied().collect();
    assert_eq!(distinct.len(), 800);
    let [a, b] = w.blocks.as_slice() else {
        return Err(TestError::new("two blocks"));
    };
    assert!(a.end() <= b.start().get() || b.end() <= a.start().get());
    assert_eq!(
        (a.start().get(), a.end(), b.start().get(), b.end()),
        (1, 501, 501, 801)
    );
    Ok(())
}

#[when("a partition allocates sequences 1 to 10 of a block 1..1000, crashes, and restarts")]
fn partition_restarts(w: &mut Steps) -> TestResult {
    w.start()?;
    let blocks = LeasedBlocks::new();
    let group = w.group()?;
    let block = supply(group, &blocks, 1, 999)?;
    assert_eq!((block.start().get(), block.end()), (1, 1_000));
    let mut allocator = AllocatorModel::new(blocks);
    let mut before = Vec::new();
    for _ in 0..10 {
        before.push(
            allocator
                .next(TypePrefix::RESOURCE)
                .map_err(TestError::new)?,
        );
    }
    w.allocated.push(before);
    // The partition crashes: its blocks and allocator are gone; the restarted one leases anew. The group restarts too.
    w.restart()?;
    let restarted_blocks = LeasedBlocks::new();
    let _block = supply(w.group()?, &restarted_blocks, 1, 999)?;
    let mut restarted = AllocatorModel::new(restarted_blocks);
    let mut after = Vec::new();
    for _ in 0..10 {
        after.push(
            restarted
                .next(TypePrefix::RESOURCE)
                .map_err(TestError::new)?,
        );
    }
    w.allocated.push(after);
    Ok(())
}

#[then(
    "it allocates no sequence it allocated before the crash, and resumes from a sequence above 10 or a new block"
)]
fn no_reuse(w: &mut Steps) -> TestResult {
    let [before, after] = w.allocated.as_slice() else {
        return Err(TestError::new("two runs"));
    };
    assert_eq!(before, &(1..=10).collect::<Vec<u64>>());
    assert!(after.iter().all(|s| !before.contains(s)));
    // The new block begins where the abandoned one ended.
    assert_eq!(after, &(1_000..1_010).collect::<Vec<u64>>());
    Ok(())
}

/// Leases, through the group, the blocks 1000..2000 and 5000..6000 for partition 1 and returns its queue holding them;
/// partition 2 takes the sequences between.
fn blocks_1000_and_5000(w: &mut Steps, both: bool) -> TestResult<LeasedBlocks> {
    w.start()?;
    let group = w.group()?;
    let ours = LeasedBlocks::new();
    let theirs = LeasedBlocks::new();
    let _first = supply(group, &theirs, 2, 999)?;
    let held = supply(group, &ours, 1, 1_000)?;
    assert_eq!((held.start().get(), held.end()), (1_000, 2_000));
    if both {
        let _between = supply(group, &theirs, 2, 3_000)?;
        let next = supply(group, &ours, 1, 1_000)?;
        assert_eq!((next.start().get(), next.end()), (5_000, 6_000));
    }
    Ok(ours)
}

/// Allocates every sequence of the block 1000..2000 from `allocator`.
fn allocate_through_1999(allocator: &mut AllocatorModel<LeasedBlocks>) -> TestResult {
    for expected in 1_000..2_000 {
        assert_eq!(
            allocator
                .next(TypePrefix::RESOURCE)
                .map_err(TestError::new)?,
            expected
        );
    }
    Ok(())
}

#[when(
    "a partition holding the block 1000..2000 and the next block 5000..6000 allocates after sequence 1999"
)]
fn after_1999_with_next(w: &mut Steps) -> TestResult {
    let blocks = blocks_1000_and_5000(w, true)?;
    let mut allocator = AllocatorModel::new(blocks);
    allocate_through_1999(&mut allocator)?;
    let next = allocator
        .next(TypePrefix::RESOURCE)
        .map_err(TestError::new)?;
    w.allocated.push(vec![next]);
    Ok(())
}

#[then("it allocates sequence 5000")]
fn allocates_5000(w: &mut Steps) -> TestResult {
    assert_eq!(w.allocated, vec![vec![5_000]]);
    Ok(())
}

#[when(
    "a partition holding only the block 1000..2000, with the metadata group unreachable, allocates after sequence 1999"
)]
fn after_1999_unreachable(w: &mut Steps) -> TestResult {
    let blocks = blocks_1000_and_5000(w, false)?;
    // Unreachable: the host supplies nothing more.
    let mut allocator = AllocatorModel::new(blocks.clone());
    allocate_through_1999(&mut allocator)?;
    let refusal = allocator
        .next(TypePrefix::RESOURCE)
        .err()
        .context("a refusal")?;
    w.allocator_refusal = Some(refusal);
    w.allocated
        .push(vec![u64::try_from(blocks.held(TypePrefix::RESOURCE))?]);
    Ok(())
}

#[then("it refuses with an error naming the `Resource` type, and allocates nothing")]
fn refuses_resource(w: &mut Steps) -> TestResult {
    let refusal = w.allocator_refusal.clone().context("the refusal")?;
    assert!(refusal.contains("Resource"), "{refusal}");
    assert_eq!(w.allocated, vec![vec![0]]);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A schema change takes effect at an epoch boundary
// ---------------------------------------------------------------------------------------------------------------------

/// A cluster whose tenant holds version 1, without the index, effective from epoch 2, with epoch 48 its latest
/// close and version 2, adding the index, committed as pending.
fn index_added_at_48() -> TestResult<Cluster> {
    let mut cluster = Cluster::new()?;
    let _without = cluster.group.offer_catalog(tenant(), 0, b"no index")?;
    sync(&mut cluster.group)?;
    let _closes = cluster.close_through(48)?;
    let _with = cluster.group.offer_catalog(tenant(), 1, b"index")?;
    sync(&mut cluster.group)?;
    Ok(cluster)
}

/// Whether a round of `epoch` validated by `partition` maintains the index's entries.
fn maintains_index(cluster: &Cluster, partition: &EpochModel, epoch: u64) -> TestResult<bool> {
    let (_, contents) = partition.validate(cluster.group.state(), tenant().get(), epoch)?;
    Ok(contents == b"index")
}

#[when(
    "an index is added while epoch 48 is the latest close, over two partitions, and the sequencer closes epoch 49"
)]
fn index_two_partitions(w: &mut Steps) -> TestResult {
    let mut cluster = index_added_at_48()?;
    let mut partitions = [EpochModel::at(48), EpochModel::at(48)];
    let close = cluster.close()?;
    for partition in &mut partitions {
        let before = partition.stamp();
        let maintained_before = maintains_index(&cluster, partition, before)?;
        w.validated.push((before, u64::from(maintained_before)));
        partition.deliver(&close);
        let after = partition.stamp();
        let maintained_after = maintains_index(&cluster, partition, after)?;
        w.validated.push((after, u64::from(maintained_after)));
    }
    w.closes.push(close);
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "the change takes effect at epoch 50, and in both partitions rounds of epoch 49 maintain no entries of the index and rounds of epoch 50 do"
)]
fn switch_at_50(w: &mut Steps) -> TestResult {
    let close = w.closes.first().context("the close of 49")?;
    assert_eq!(close.epoch().get(), 49);
    let state = w.cluster()?.group.state();
    let record = state
        .catalog_at(tenant(), epoch(50)?)?
        .context("the version")?;
    assert_eq!(
        (record.version(), record.effective().map(Epoch::get)),
        (2, Some(50))
    );
    // Per partition: epoch 49 without the index, epoch 50 with it.
    assert_eq!(w.validated, vec![(49, 0), (50, 1), (49, 0), (50, 1)]);
    Ok(())
}

#[when(
    "an index is added while epoch 48 is the latest close, and a partition's latest delivered close is 48"
)]
fn index_partition_at_48(w: &mut Steps) -> TestResult {
    let mut cluster = index_added_at_48()?;
    let mut partition = EpochModel::at(48);
    // Before the close of 49, the group answers no epoch past 49.
    let ahead = cluster
        .group
        .state()
        .version_at(tenant().get(), 50)
        .err()
        .context("epoch 50 not yet answered")?;
    w.refusals.push(ahead);
    w.stamps.push(partition.stamp());
    let close = cluster.close()?;
    // The close is durable and the group has learned it, but the partition has not received it yet.
    w.stamps.push(partition.stamp());
    partition.deliver(&close);
    let first = partition.stamp();
    w.stamps.push(first);
    let (version, contents) = partition.validate(cluster.group.state(), tenant().get(), first)?;
    assert_eq!(contents, b"index");
    w.validated.push((first, version));
    w.closes.push(close);
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "the partition's commits carry epoch 49 until the close of 49, carrying the new version, is delivered to it, and its first commit at epoch 50 is validated against the new version"
)]
fn holds_version_before_50(w: &mut Steps) -> TestResult {
    assert_eq!(w.stamps, vec![49, 49, 50]);
    let close = w.closes.first().context("the close of 49")?;
    let announced: Vec<u64> = close.catalog().iter().map(|v| v.version()).collect();
    assert_eq!(announced, vec![2]);
    assert_eq!(w.validated, vec![(50, 2)]);
    let ahead = w.refusals.first().context("the refusal")?;
    assert_eq!(
        ahead.0,
        ClusterErrorEnum::EpochAhead {
            epoch: 50,
            latest: 49
        }
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
