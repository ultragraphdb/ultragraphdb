/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId, PlacementMap, TenantRun};
use ultragraphdb_common_compute::RoleLinks;
use ultragraphdb_common_role::{RoleCluster, RoleSecret};
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_protocol_wire::Join;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;

/// A cluster whose role secret is in a file of `name`.
fn cluster(name: &str) -> TestResult<RoleCluster> {
    let file = std::env::temp_dir().join(format!("role-links-{name}-{}", std::process::id()));
    std::fs::write(&file, "a role secret")?;
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        Duration::from_secs(1),
    ))
}

/// The links hold no placement map, and so no run or partition, until they are given one, and then hold that one.
#[test]
fn test_the_links_hold_the_map_they_were_given() -> TestResult {
    let links = RoleLinks::new(cluster("map")?, "compute".to_string());
    assert_eq!(links.map(), None);
    assert!(links.run_partitions(0).is_empty(), "no run without a map");
    assert!(
        links.every_partition().is_empty(),
        "no partition without a map"
    );
    let map = PlacementMap::even(2, NodeId::new(1))?;
    links.set_map(map.clone());
    assert_eq!(links.map(), Some(map));
    Ok(())
}

/// A link to a role whose connection closed is dialled anew by the next request for it, never handed out closed.
#[test]
fn test_a_closed_link_is_dialled_anew() -> TestResult {
    let cluster = cluster("closed")?;
    // A port nothing listens on: each dial to it is refused.
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    Runtime::new()?.block_on(async {
        let links = RoleLinks::new(cluster, "compute".to_string());
        links.joined(&Join::storage(format!("tcp://127.0.0.1:{port}"), vec![1]));
        let first = links.partition(1).context("the partition's link")?;
        let mut waited = 0;
        while !first.is_closed() {
            if waited == 5_000 {
                return Err(TestError::new("the refused dial did not close the link"));
            }
            compio::time::sleep(Duration::from_millis(1)).await;
            waited += 1;
        }
        let again = links.partition(1).context("the partition's link")?;
        assert!(!again.is_closed(), "dialled anew");
        Ok(())
    })
}

/// The links list the partitions joined, ascending, as the last join for each says; a compute node's Join lists none.
#[test]
fn test_the_links_list_the_partitions_joined() -> TestResult {
    let links = RoleLinks::new(cluster("joined")?, "compute".to_string());
    assert!(links.partitions().is_empty());
    links.joined(&Join::compute("tcp://127.0.0.1:1"));
    assert!(
        links.partitions().is_empty(),
        "a compute node holds no partition"
    );
    links.joined(&Join::storage("tcp://127.0.0.1:3", vec![3, 1]));
    links.joined(&Join::storage("tcp://127.0.0.1:4", vec![2]));
    assert_eq!(links.partitions(), vec![1, 2, 3]);
    Ok(())
}

/// A tenant's run is held by the partitions whose ranges overlap it, as the placement's own rule says, and by no partition whose
/// range only touches it.
#[test]
fn test_a_runs_partitions_are_those_overlapping_it() -> TestResult {
    let links = RoleLinks::new(cluster("run")?, "compute".to_string());
    // The first run on partition 1, every other on partition 2: the second's range starts where the first run ends.
    let mut map = PlacementMap::even(1, NodeId::new(1))?;
    let first_run = TenantRun::new(0).range();
    map.change(
        Epoch::new(1)?,
        &[
            (first_run, PartitionId::new(1)?),
            (
                BucketRange::new(first_run.end(), BucketRange::END)?,
                PartitionId::new(2)?,
            ),
        ],
    )?;
    links.set_map(map.clone());
    let mut first_runs = 0;
    for tenant in 0..1_024 {
        first_runs += usize::from(TenantRun::of(Tenant::new(tenant)) == TenantRun::new(0));
        let holders: Vec<u32> = map
            .holders_of_run(Tenant::new(tenant), map.latest_change())
            .into_iter()
            .map(|(partition, _)| partition.get())
            .collect();
        assert_eq!(links.run_partitions(tenant), holders, "tenant {tenant}");
    }
    assert!(first_runs > 0, "a tenant of the first run");
    Ok(())
}
