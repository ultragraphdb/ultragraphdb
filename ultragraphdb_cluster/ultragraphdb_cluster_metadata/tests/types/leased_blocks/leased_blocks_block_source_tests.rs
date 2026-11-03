/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock};

#[test]
fn test_blocks_are_handed_out_oldest_first_and_once() -> TestResult {
    let blocks = LeasedBlocks::new();
    let first = SequenceBlock::new(1, 1_001).context("a block")?;
    let second = SequenceBlock::new(5_000, 6_000).context("a block")?;
    blocks.supply(TypePrefix::RESOURCE, first)?;
    blocks.supply(TypePrefix::RESOURCE, second)?;
    assert_eq!(blocks.lease(TypePrefix::RESOURCE), Some(first));
    assert_eq!(blocks.held(TypePrefix::RESOURCE), 1);
    assert_eq!(blocks.lease(TypePrefix::RESOURCE), Some(second));
    assert_eq!(blocks.lease(TypePrefix::RESOURCE), None);
    Ok(())
}

#[test]
fn test_a_store_and_its_host_share_the_queue() -> TestResult {
    // The store holds one clone and leases from it; the host supplies through another.
    let store = LeasedBlocks::new();
    let host = store.clone();
    let block = SequenceBlock::new(7, 9).context("a block")?;
    host.supply(TypePrefix::RESOURCE, block)?;
    assert_eq!(store.lease(TypePrefix::RESOURCE), Some(block));
    assert_eq!(host.held(TypePrefix::RESOURCE), 0);
    Ok(())
}

#[test]
fn test_a_type_with_no_block_leases_nothing() -> TestResult {
    let blocks = LeasedBlocks::new();
    blocks.supply(
        TypePrefix::STATEMENT,
        SequenceBlock::new(1, 2).context("a block")?,
    )?;
    assert_eq!(blocks.lease(TypePrefix::RESOURCE), None);
    assert_eq!(blocks.held(TypePrefix::STATEMENT), 1);
    Ok(())
}
