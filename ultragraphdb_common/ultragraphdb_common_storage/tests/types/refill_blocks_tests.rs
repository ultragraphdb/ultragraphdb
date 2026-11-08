/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::mpsc::unbounded;
use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_common_storage::RefillBlocks;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock};

/// A lease hands out the block the metadata group leased, at once, and notes the type taken for the core, which leases the next.
#[test]
fn test_a_lease_hands_out_the_leased_block_and_notes_its_type() -> TestResult {
    let leased = LeasedBlocks::new();
    let block = SequenceBlock::new(10, 20).context("a block of ten sequences")?;
    leased.supply(TypePrefix::STATEMENT, block)?;
    let (wanted, mut wants) = unbounded();
    let blocks = RefillBlocks::new(leased.clone(), wanted);
    assert_eq!(blocks.lease(TypePrefix::STATEMENT), Some(block));
    assert_eq!(leased.held(TypePrefix::STATEMENT), 0);
    assert_eq!(wants.try_recv().ok(), Some(TypePrefix::STATEMENT));
    Ok(())
}

/// A type with no block leased hands out none, and still asks the core for one.
#[test]
fn test_a_type_with_no_block_leases_none_and_asks_for_one() -> TestResult {
    let (wanted, mut wants) = unbounded();
    let blocks = RefillBlocks::new(LeasedBlocks::new(), wanted);
    assert_eq!(blocks.lease(TypePrefix::RESOURCE), None);
    assert_eq!(wants.try_recv().ok(), Some(TypePrefix::RESOURCE));
    Ok(())
}
