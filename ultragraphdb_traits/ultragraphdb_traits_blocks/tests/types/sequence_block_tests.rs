/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Sequence;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::SequenceBlock;

fn seq(n: u64) -> TestResult<Sequence> {
    Sequence::new(n).context("a sequence above 0")
}

#[test]
fn test_a_block_holds_its_start_and_ends_before_its_end() -> TestResult {
    let block = SequenceBlock::new(1000, 2000).context("1000..2000")?;
    assert_eq!(block.start(), seq(1000)?);
    assert_eq!(block.end(), 2000);
    // 2000 - 1000, counted by hand.
    assert_eq!(block.count(), 1000);
    assert!(block.contains(seq(1000)?));
    assert!(block.contains(seq(1999)?));
    assert!(!block.contains(seq(2000)?));
    assert!(!block.contains(seq(999)?));
    Ok(())
}

#[test]
fn test_a_block_of_one_sequence() -> TestResult {
    let block = SequenceBlock::new(1, 2).context("1..2")?;
    assert_eq!(block.count(), 1);
    assert!(block.contains(seq(1)?));
    assert!(!block.contains(seq(2)?));
    let last = SequenceBlock::new(u64::MAX - 1, u64::MAX).context("the last block")?;
    assert_eq!(last.count(), 1);
    assert!(last.contains(seq(u64::MAX - 1)?));
    assert!(!last.contains(seq(u64::MAX)?));
    Ok(())
}

#[test]
fn test_the_widest_block() -> TestResult {
    let block = SequenceBlock::new(1, u64::MAX).context("1..2^64 - 1")?;
    assert_eq!(block.count(), u64::MAX - 1);
    assert!(block.contains(seq(u64::MAX - 1)?));
    Ok(())
}

#[test]
fn test_an_empty_or_zero_block_is_refused() {
    assert_eq!(SequenceBlock::new(0, 10), None);
    assert_eq!(SequenceBlock::new(0, 0), None);
    assert_eq!(SequenceBlock::new(5, 5), None);
    assert_eq!(SequenceBlock::new(6, 5), None);
    assert_eq!(SequenceBlock::new(u64::MAX, u64::MAX), None);
}
