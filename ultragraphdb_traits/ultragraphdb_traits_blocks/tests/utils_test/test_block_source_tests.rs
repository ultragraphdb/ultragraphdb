/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock, TestBlockSource};

fn len(n: u64) -> TestResult<NonZeroU64> {
    NonZeroU64::new(n).context("a block length above 0")
}

fn block(start: u64, end: u64) -> TestResult<SequenceBlock> {
    SequenceBlock::new(start, end).context("a valid block")
}

#[test]
fn test_counted_blocks_follow_each_other_from_one() -> TestResult {
    let source = TestBlockSource::new(len(1000)?);
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(1, 1001)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(1001, 2001)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(2001, 3001)?));
    Ok(())
}

#[test]
fn test_each_type_counts_on_its_own() -> TestResult {
    let source = TestBlockSource::new(len(5)?);
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(1, 6)?));
    assert_eq!(source.lease(TypePrefix::STATEMENT), Some(block(1, 6)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(6, 11)?));
    assert_eq!(
        source.leased(TypePrefix::RESOURCE),
        vec![block(1, 6)?, block(6, 11)?]
    );
    assert_eq!(source.leased(TypePrefix::STATEMENT), vec![block(1, 6)?]);
    Ok(())
}

#[test]
fn test_a_block_of_one_sequence_is_leased() -> TestResult {
    let source = TestBlockSource::new(NonZeroU64::MIN);
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(1, 2)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(2, 3)?));
    Ok(())
}

#[test]
fn test_spent_sequences_lease_nothing() -> TestResult {
    let source = TestBlockSource::new(len(u64::MAX - 1)?);
    assert_eq!(
        source.lease(TypePrefix::RESOURCE),
        Some(block(1, u64::MAX)?)
    );
    assert_eq!(source.lease(TypePrefix::RESOURCE), None);
    assert_eq!(source.lease(TypePrefix::RESOURCE), None);
    assert_eq!(
        source.leased(TypePrefix::RESOURCE),
        vec![block(1, u64::MAX)?]
    );
    Ok(())
}

#[test]
fn test_an_unreachable_source_leases_nothing_and_resumes_where_it_was() -> TestResult {
    let source = TestBlockSource::new(len(10)?);
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(1, 11)?));
    source.set_reachable(false);
    assert_eq!(source.lease(TypePrefix::RESOURCE), None);
    assert_eq!(source.lease(TypePrefix::STATEMENT), None);
    source.set_reachable(true);
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(11, 21)?));
    assert_eq!(
        source.leased(TypePrefix::RESOURCE),
        vec![block(1, 11)?, block(11, 21)?]
    );
    Ok(())
}

#[test]
fn test_scripted_blocks_are_leased_in_order_and_then_none() -> TestResult {
    let source = TestBlockSource::new(len(10)?).with_blocks(
        TypePrefix::RESOURCE,
        vec![block(1000, 2000)?, block(5000, 6000)?],
    );
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(1000, 2000)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(5000, 6000)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), None);
    // Another type still counts.
    assert_eq!(source.lease(TypePrefix::STATEMENT), Some(block(1, 11)?));
    Ok(())
}

#[test]
fn test_an_empty_script_leases_nothing() -> TestResult {
    let source = TestBlockSource::new(len(10)?).with_blocks(TypePrefix::STATEMENT, Vec::new());
    assert_eq!(source.lease(TypePrefix::STATEMENT), None);
    assert!(source.leased(TypePrefix::STATEMENT).is_empty());
    Ok(())
}

#[test]
fn test_an_unreachable_script_keeps_its_blocks() -> TestResult {
    let source =
        TestBlockSource::new(len(10)?).with_blocks(TypePrefix::RESOURCE, vec![block(7, 9)?]);
    source.set_reachable(false);
    assert_eq!(source.lease(TypePrefix::RESOURCE), None);
    source.set_reachable(true);
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(7, 9)?));
    Ok(())
}

#[test]
fn test_clones_share_one_source() -> TestResult {
    let source = TestBlockSource::new(len(3)?);
    let clone = source.clone();
    assert_eq!(clone.lease(TypePrefix::RESOURCE), Some(block(1, 4)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(4, 7)?));
    clone.set_reachable(false);
    assert_eq!(source.lease(TypePrefix::RESOURCE), None);
    assert_eq!(
        source.leased(TypePrefix::RESOURCE),
        clone.leased(TypePrefix::RESOURCE)
    );
    Ok(())
}

#[test]
fn test_a_block_reaching_past_the_last_sequence_is_not_cut_short() -> TestResult {
    // 2^63 + 1 sequences a block: the first is 1..2^63 + 2; the second would end past 2^64 - 1, so none is leased
    // rather than a shorter one.
    let source = TestBlockSource::new(len((1 << 63) + 1)?);
    assert_eq!(
        source.lease(TypePrefix::RESOURCE),
        Some(block(1, (1 << 63) + 2)?)
    );
    assert_eq!(source.lease(TypePrefix::RESOURCE), None);
    Ok(())
}

#[test]
fn test_a_scripted_block_below_an_earlier_one_is_leased() -> TestResult {
    // A source owes no order, only disjoint blocks: a later block may lie below an earlier one.
    let source = TestBlockSource::new(len(10)?).with_blocks(
        TypePrefix::RESOURCE,
        vec![block(5000, 6000)?, block(1000, 2000)?],
    );
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(5000, 6000)?));
    assert_eq!(source.lease(TypePrefix::RESOURCE), Some(block(1000, 2000)?));
    Ok(())
}
