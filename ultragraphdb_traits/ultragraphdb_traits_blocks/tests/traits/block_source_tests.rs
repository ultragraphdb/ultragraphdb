/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock, TestBlockSource};

/// The contract every source keeps: `leases` blocks of `prefix` leased in turn overlap none another.
fn assert_disjoint<B: BlockSource>(
    source: &B,
    prefix: TypePrefix,
    leases: usize,
) -> TestResult<Vec<SequenceBlock>> {
    let mut blocks: Vec<SequenceBlock> = Vec::new();
    for _ in 0..leases {
        let block = source.lease(prefix).context("a reachable source leases")?;
        for held in &blocks {
            assert!(
                block.start().get() >= held.end() || block.end() <= held.start().get(),
                "{block:?} overlaps {held:?}"
            );
        }
        blocks.push(block);
    }
    Ok(blocks)
}

#[test]
fn test_a_source_never_leases_one_sequence_twice() -> TestResult {
    let source = TestBlockSource::new(NonZeroU64::MIN.saturating_add(9));
    let blocks = assert_disjoint(&source, TypePrefix::RESOURCE, 5)?;
    // Five blocks of 10 from 1, by hand: 1..11, 11..21, ..., 41..51.
    assert_eq!(blocks.last().copied(), SequenceBlock::new(41, 51));
    Ok(())
}

#[test]
fn test_two_partitions_of_one_source_never_collide() -> TestResult {
    // The system-identity scenario "Two partitions never collide", over the double: two partitions' handles on one
    // metadata group lease in turn, and every sequence of every block is allocated once.
    let partition_1 = TestBlockSource::new(NonZeroU64::MIN.saturating_add(99));
    let partition_2 = partition_1.clone();
    let a = partition_1
        .lease(TypePrefix::RESOURCE)
        .context("partition 1 leases")?;
    let b = partition_2
        .lease(TypePrefix::RESOURCE)
        .context("partition 2 leases")?;
    let mut sequences: Vec<u64> = (a.start().get()..a.end())
        .chain(b.start().get()..b.end())
        .collect();
    let all = sequences.len();
    sequences.sort_unstable();
    sequences.dedup();
    assert_eq!(sequences.len(), all, "no sequence is allocated twice");
    // 100 + 100 sequences, by hand.
    assert_eq!(all, 200);
    Ok(())
}
