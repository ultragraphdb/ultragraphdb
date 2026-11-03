/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::SequenceBlock;

/// The block `start..end`.
fn block(start: u64, end: u64) -> TestResult<SequenceBlock> {
    SequenceBlock::new(start, end).context("a block")
}

#[test]
fn test_a_new_queue_holds_nothing() {
    let blocks = LeasedBlocks::new();
    assert_eq!(blocks.held(TypePrefix::RESOURCE), 0);
    assert_eq!(LeasedBlocks::default().held(TypePrefix::STATEMENT), 0);
    assert_eq!(LeasedBlocks::HELD_MAX, 16);
}

#[test]
fn test_blocks_are_counted_per_type() -> TestResult {
    let blocks = LeasedBlocks::new();
    blocks.supply(TypePrefix::RESOURCE, block(1, 10)?)?;
    blocks.supply(TypePrefix::RESOURCE, block(10, 20)?)?;
    blocks.supply(TypePrefix::STATEMENT, block(1, 5)?)?;
    assert_eq!(blocks.held(TypePrefix::RESOURCE), 2);
    assert_eq!(blocks.held(TypePrefix::STATEMENT), 1);
    assert_eq!(blocks.held(TypePrefix::new(3)?), 0);
    Ok(())
}

#[test]
fn test_a_type_holding_the_most_blocks_takes_no_more() -> TestResult {
    let blocks = LeasedBlocks::new();
    for start in 1..=16 {
        blocks.supply(TypePrefix::RESOURCE, block(start * 10, start * 10 + 1)?)?;
    }
    let refusal = blocks
        .supply(TypePrefix::RESOURCE, block(500, 501)?)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::Full {
            what: "blocks of one type held",
            capacity: 16
        }
    );
    assert_eq!(blocks.held(TypePrefix::RESOURCE), 16);
    // Another type is not full.
    blocks.supply(TypePrefix::STATEMENT, block(1, 2)?)?;
    Ok(())
}

#[test]
fn test_clones_share_one_queue() -> TestResult {
    let host = LeasedBlocks::new();
    let store = host.clone();
    host.supply(TypePrefix::RESOURCE, block(1, 2)?)?;
    assert_eq!(store.held(TypePrefix::RESOURCE), 1);
    Ok(())
}
