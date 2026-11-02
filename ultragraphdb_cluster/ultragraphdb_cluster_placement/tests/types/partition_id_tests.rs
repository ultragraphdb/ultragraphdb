/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_partition_zero_is_refused() -> TestResult {
    let refusal = PartitionId::new(0).err().context("a refusal")?;
    assert_eq!(refusal.0, ClusterErrorEnum::ZeroPartition);
    Ok(())
}

#[test]
fn test_partitions_one_and_the_largest_hold_their_value() -> TestResult {
    assert_eq!(PartitionId::new(1)?.get(), 1);
    assert_eq!(PartitionId::new(u32::MAX)?.get(), u32::MAX);
    Ok(())
}

#[test]
fn test_partitions_order_by_number() -> TestResult {
    assert!(PartitionId::new(2)? < PartitionId::new(3)?);
    assert_ne!(PartitionId::new(2)?, PartitionId::new(3)?);
    Ok(())
}
