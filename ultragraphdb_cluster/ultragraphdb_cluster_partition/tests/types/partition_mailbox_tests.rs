/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::PartitionMailbox;
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A mesh gives each partition a mailbox naming it, with a sender to every partition and none to another.
#[test]
fn test_a_mesh_connects_every_partition_and_no_other() -> TestResult {
    let ids = [PartitionId::new(1)?, PartitionId::new(2)?];
    let mesh = PartitionMailbox::mesh(&ids);
    assert_eq!(
        mesh.iter().map(PartitionMailbox::id).collect::<Vec<_>>(),
        ids
    );
    let first = mesh.first().context("a mailbox")?;
    assert!(ids.iter().all(|id| first.sender(*id).is_some()));
    assert!(first.sender(PartitionId::new(3)?).is_none());
    Ok(())
}
