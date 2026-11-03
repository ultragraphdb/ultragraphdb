/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_store_quads::{QuadErrorEnum, Sequences};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock, TestBlockSource};

fn len(n: u64) -> TestResult<NonZeroU64> {
    NonZeroU64::new(n).context("a block length above 0")
}

fn block(start: u64, end: u64) -> TestResult<SequenceBlock> {
    SequenceBlock::new(start, end).context("a valid block")
}

fn next(s: &mut Sequences<TestBlockSource>, prefix: TypePrefix) -> TestResult<u64> {
    Ok(s.next(prefix)?.get())
}

#[test]
fn test_sequences_run_in_order_from_the_first_block() -> TestResult {
    let source = TestBlockSource::new(len(1000)?);
    let mut sequences = Sequences::new(source.clone());
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 1);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 2);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 3);
    Ok(())
}

#[test]
fn test_the_next_block_is_leased_once_the_current_one_is_begun() -> TestResult {
    let source = TestBlockSource::new(len(3)?);
    let mut sequences = Sequences::new(source.clone());
    assert!(
        source.leased(TypePrefix::RESOURCE).is_empty(),
        "nothing before the first allocation"
    );
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 1);
    assert_eq!(
        source.leased(TypePrefix::RESOURCE),
        vec![block(1, 4)?, block(4, 7)?]
    );
    // 2 and 3 finish the block; 4 begins the one ahead, and the block after it is leased.
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 2);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 3);
    assert_eq!(source.leased(TypePrefix::RESOURCE).len(), 2);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 4);
    assert_eq!(source.leased(TypePrefix::RESOURCE).len(), 3);
    Ok(())
}

#[test]
fn test_a_used_up_block_gives_way_to_the_next() -> TestResult {
    // The system-identity scenario: holding 1000..2000 and the next block 5000..6000, after 1999 comes 5000.
    let source = TestBlockSource::new(len(10)?).with_blocks(
        TypePrefix::RESOURCE,
        vec![block(1000, 2000)?, block(5000, 6000)?],
    );
    let mut sequences = Sequences::new(source);
    let first: Vec<u64> = (0..1000)
        .map(|_| next(&mut sequences, TypePrefix::RESOURCE))
        .collect::<TestResult<_>>()?;
    assert_eq!(first.first(), Some(&1000));
    assert_eq!(first.last(), Some(&1999));
    assert!(
        first.windows(2).all(|p| p[1] == p[0] + 1),
        "in order, none skipped"
    );
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 5000);
    Ok(())
}

#[test]
fn test_a_sequence_outside_a_held_block_is_refused_naming_the_type() -> TestResult {
    // The system-identity scenario: holding only 1000..2000 with the metadata group unreachable, the allocation after
    // 1999 is refused naming the Resource type, and nothing is allocated.
    let source =
        TestBlockSource::new(len(10)?).with_blocks(TypePrefix::RESOURCE, vec![block(1000, 2000)?]);
    let mut sequences = Sequences::new(source);
    for _ in 0..1000 {
        let _allocated: u64 = next(&mut sequences, TypePrefix::RESOURCE)?;
    }
    for _ in 0..2 {
        let error = sequences
            .next(TypePrefix::RESOURCE)
            .err()
            .context("refused")?;
        assert_eq!(error.0, QuadErrorEnum::NoSequence { prefix: 1 });
        assert!(error.to_string().contains("Resource"), "{error}");
    }
    Ok(())
}

#[test]
fn test_an_unreachable_source_refuses_the_first_sequence_and_a_statement_is_named() -> TestResult {
    let source = TestBlockSource::new(len(10)?);
    source.set_reachable(false);
    let mut sequences = Sequences::new(source.clone());
    let error = sequences
        .next(TypePrefix::STATEMENT)
        .err()
        .context("refused")?;
    assert_eq!(error.0, QuadErrorEnum::NoSequence { prefix: 2 });
    assert!(error.to_string().contains("Statement"), "{error}");
    source.set_reachable(true);
    assert_eq!(
        next(&mut sequences, TypePrefix::STATEMENT)?,
        1,
        "nothing was spent by the refusal"
    );
    Ok(())
}

#[test]
fn test_a_declared_type_is_named_by_its_prefix() -> TestResult {
    let source = TestBlockSource::new(len(10)?);
    source.set_reachable(false);
    let mut sequences = Sequences::new(source);
    let prefix = TypePrefix::new(7)?;
    let error = sequences.next(prefix).err().context("refused")?;
    assert_eq!(error.0, QuadErrorEnum::NoSequence { prefix: 7 });
    assert!(error.to_string().contains("type 7"), "{error}");
    Ok(())
}

#[test]
fn test_an_unreachable_source_leaves_the_held_blocks_usable() -> TestResult {
    let source = TestBlockSource::new(len(2)?);
    let mut sequences = Sequences::new(source.clone());
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 1);
    source.set_reachable(false);
    // The current block 1..3 and the block ahead 3..5 are held.
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 2);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 3);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 4);
    assert!(
        sequences.next(TypePrefix::RESOURCE).is_err(),
        "nothing past the blocks held"
    );
    source.set_reachable(true);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 5);
    Ok(())
}

#[test]
fn test_each_type_has_its_own_sequences() -> TestResult {
    let source = TestBlockSource::new(len(100)?);
    let mut sequences = Sequences::new(source);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 1);
    assert_eq!(next(&mut sequences, TypePrefix::STATEMENT)?, 1);
    assert_eq!(next(&mut sequences, TypePrefix::RESOURCE)?, 2);
    assert_eq!(next(&mut sequences, TypePrefix::STATEMENT)?, 2);
    Ok(())
}

#[test]
fn test_blocks_of_one_sequence() -> TestResult {
    let source = TestBlockSource::new(NonZeroU64::MIN);
    let mut sequences = Sequences::new(source);
    let all: Vec<u64> = (0..5)
        .map(|_| next(&mut sequences, TypePrefix::RESOURCE))
        .collect::<TestResult<_>>()?;
    assert_eq!(all, vec![1, 2, 3, 4, 5]);
    Ok(())
}

#[test]
fn test_a_restarted_partition_reuses_no_sequence() -> TestResult {
    // The system-identity scenario: 1 to 10 of the block 1..1000 allocated, a crash, a restart: no sequence allocated
    // before is allocated again, and allocation resumes above 10.
    let source = TestBlockSource::new(len(999)?);
    let mut before = Sequences::new(source.clone());
    let first: BTreeSet<u64> = (0..10)
        .map(|_| next(&mut before, TypePrefix::RESOURCE))
        .collect::<TestResult<_>>()?;
    assert_eq!(first, (1..=10).collect());
    drop(before);
    let mut after = Sequences::new(source.clone());
    let resumed = next(&mut after, TypePrefix::RESOURCE)?;
    assert!(resumed > 10, "{resumed}");
    assert!(!first.contains(&resumed));
    assert_eq!(
        after.blocks().leased(TypePrefix::RESOURCE).len(),
        source.leased(TypePrefix::RESOURCE).len()
    );
    Ok(())
}

#[test]
fn test_two_partitions_of_one_source_never_collide() -> TestResult {
    let source = TestBlockSource::new(len(7)?);
    let mut one = Sequences::new(source.clone());
    let mut two = Sequences::new(source.clone());
    let mut seen = BTreeSet::new();
    for _ in 0..50 {
        assert!(seen.insert(next(&mut one, TypePrefix::RESOURCE)?));
        assert!(seen.insert(next(&mut two, TypePrefix::RESOURCE)?));
    }
    assert!(source.lease(TypePrefix::RESOURCE).is_some());
    Ok(())
}
