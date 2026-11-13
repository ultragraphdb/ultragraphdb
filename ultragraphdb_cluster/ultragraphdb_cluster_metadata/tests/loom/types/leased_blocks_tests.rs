/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use loom::thread;
use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock};

/// Two takers leasing at once from a queue holding one block: the block is leased to exactly one of them, so no two
/// stores ever allocate the same sequences, and the queue ends holding none.
#[test]
fn test_two_leasing_the_last_block_take_it_once() -> TestResult {
    let prefix = TypePrefix::new(3)?;
    let block = SequenceBlock::new(1, 101).context("a block")?;
    loom::model(move || {
        let blocks = LeasedBlocks::new();
        assert!(blocks.supply(prefix, block).is_ok());
        let other = blocks.clone();
        let theirs = thread::spawn(move || other.lease(prefix));
        let mine = blocks.lease(prefix);
        let theirs = theirs.join();
        assert!(theirs.is_ok(), "the other taker");
        let leased = [mine, theirs.ok().flatten()];
        assert_eq!(leased.iter().flatten().count(), 1, "{leased:?}");
        assert!(leased.iter().flatten().all(|taken| *taken == block));
        assert_eq!(blocks.held(prefix), 0);
    });
    Ok(())
}

/// The host supplying a block while the store leases: the store takes that block or none, and the block is then held
/// or leased, exactly once either way.
#[test]
fn test_a_block_supplied_as_the_store_leases_is_held_or_leased_once() -> TestResult {
    let prefix = TypePrefix::new(3)?;
    let block = SequenceBlock::new(1, 101).context("a block")?;
    loom::model(move || {
        let blocks = LeasedBlocks::new();
        let host = blocks.clone();
        let supplying = thread::spawn(move || host.supply(prefix, block).is_ok());
        let leased = blocks.lease(prefix);
        assert!(supplying.join().is_ok_and(|supplied| supplied));
        assert!(leased.is_none_or(|taken| taken == block));
        assert_eq!(blocks.held(prefix) + usize::from(leased.is_some()), 1);
    });
    Ok(())
}

/// Two blocks supplied from two threads at once: both are held, and leasing hands each out once.
#[test]
fn test_two_blocks_supplied_at_once_are_both_held() -> TestResult {
    let prefix = TypePrefix::new(3)?;
    let first = SequenceBlock::new(1, 101).context("a block")?;
    let second = SequenceBlock::new(101, 201).context("a block")?;
    loom::model(move || {
        let blocks = LeasedBlocks::new();
        let host = blocks.clone();
        let supplying = thread::spawn(move || host.supply(prefix, first).is_ok());
        assert!(blocks.supply(prefix, second).is_ok());
        assert!(supplying.join().is_ok_and(|supplied| supplied));
        assert_eq!(blocks.held(prefix), 2);
        let leased = [blocks.lease(prefix), blocks.lease(prefix)];
        assert!(
            leased.contains(&Some(first)) && leased.contains(&Some(second)),
            "{leased:?}"
        );
        assert_eq!(blocks.lease(prefix), None);
    });
    Ok(())
}
