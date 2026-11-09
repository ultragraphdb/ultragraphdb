/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_frontend::{JoinFanOut, RoleLinks};
use ultragraphdb_common_role::{RoleCluster, RoleSecret};
use ultragraphdb_protocol_wire::Join;
use ultragraphdb_tool_bdd::{TestError, TestResult};

const CLUSTER: u64 = 7;
/// How long a core's links have to hear a join.
const DUE: Duration = Duration::from_secs(5);

/// A cluster whose role secret is in a file of `name`.
fn cluster(name: &str) -> TestResult<RoleCluster> {
    let file = std::env::temp_dir().join(format!("join-fan-out-{name}-{}", std::process::id()));
    std::fs::write(&file, "a role secret")?;
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        Duration::from_secs(1),
    ))
}

/// Waits until `links` hold `partitions`, within [`DUE`].
async fn until_holding(links: &RoleLinks, partitions: &[u32]) -> TestResult {
    let mut waited = Duration::ZERO;
    while links.partitions() != partitions {
        if waited >= DUE {
            return Err(TestError::new(format!(
                "the core's links hold {:?}, not {partitions:?}",
                links.partitions()
            )));
        }
        compio::time::sleep(Duration::from_millis(1)).await;
        waited += Duration::from_millis(1);
    }
    Ok(())
}

/// Every core the fan-out reaches applies each join sent to its own links, in the order sent, so a partition joining after the
/// start is routed to from every core (design D14).
#[test]
fn test_every_core_hears_each_join() -> TestResult {
    let cluster = cluster("every")?;
    Runtime::new()?.block_on(async {
        let mut fan_out = JoinFanOut::new();
        let cores: Vec<RoleLinks> = (0..3)
            .map(|_| {
                let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
                spawn(fan_out.add_core().follow(links.clone())).detach();
                links
            })
            .collect();
        assert_eq!(fan_out.cores(), 3);
        fan_out.send(&Join::storage("tcp://127.0.0.1:1", vec![1, 2]));
        fan_out.send(&Join::storage("tcp://127.0.0.1:2", vec![2, 3]));
        for links in &cores {
            until_holding(links, &[1, 2, 3]).await?;
        }
        Ok(())
    })
}

/// A fan-out whose cores ended sends on without failing, as a stopping frontend's does.
#[test]
fn test_a_join_to_ended_cores_is_dropped() {
    let mut fan_out = JoinFanOut::new();
    drop(fan_out.add_core());
    fan_out.send(&Join::compute("tcp://127.0.0.1:1"));
    assert_eq!(fan_out.cores(), 1);
}
