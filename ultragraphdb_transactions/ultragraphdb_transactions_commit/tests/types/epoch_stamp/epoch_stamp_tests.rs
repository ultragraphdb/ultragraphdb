/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::EpochStamp;

/// 2^22 − 2, the last even sequence of an epoch, by hand: 4,194,304 − 2.
const LAST_SEQUENCE: u32 = 4_194_302;

fn epoch(n: u64) -> TestResult<Epoch> {
    Epoch::new(n).context("the epoch")
}

fn version(e: u64, sequence: u32) -> TestResult<Version> {
    Version::commit(epoch(e)?, sequence).context("the version")
}

#[test]
fn test_the_first_version_is_the_open_epoch_at_sequence_0() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(9)?);
    // 10 << 22 = 10 × 4,194,304 = 41,943,040.
    assert_eq!(stamp.take().map(Version::to_u64), Some(41_943_040));
    Ok(())
}

#[test]
fn test_versions_step_by_two_within_the_epoch() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(10)?);
    let sequences: Vec<u32> = (0..3)
        .filter_map(|_| stamp.take())
        .map(Version::sequence)
        .collect();
    assert_eq!(sequences, vec![0, 2, 4]);
    assert_eq!(stamp.last(), Some(version(11, 4)?));
    Ok(())
}

#[test]
fn test_a_stamp_before_its_first_entry_has_no_last_version() -> TestResult {
    let stamp = EpochStamp::new(epoch(3)?);
    assert_eq!(stamp.last(), None);
    assert_eq!(stamp.delivered(), epoch(3)?);
    assert_eq!(stamp.open(), Some(epoch(4)?));
    assert!(!stamp.is_full());
    Ok(())
}

#[test]
fn test_before_the_first_close_the_open_epoch_is_1() -> TestResult {
    let mut stamp = EpochStamp::new(Epoch::ZERO);
    // 1 << 22 = 4,194,304.
    assert_eq!(stamp.take().map(Version::to_u64), Some(4_194_304));
    Ok(())
}

#[test]
fn test_a_full_epoch_gives_no_version_until_the_next_close() -> TestResult {
    let mut stamp = EpochStamp::after(epoch(9)?, version(10, LAST_SEQUENCE)?);
    assert!(stamp.is_full());
    assert_eq!(stamp.take(), None);
    assert_eq!(stamp.take(), None, "it never advances on its own");
    assert_eq!(stamp.open(), Some(epoch(10)?));
    assert_eq!(stamp.deliver(epoch(10)?), Some(true));
    assert!(!stamp.is_full());
    assert_eq!(stamp.take(), Some(version(11, 0)?));
    Ok(())
}

#[test]
fn test_the_last_sequence_of_an_epoch_is_taken() -> TestResult {
    let mut stamp = EpochStamp::after(epoch(9)?, version(10, LAST_SEQUENCE - 2)?);
    assert!(!stamp.is_full());
    assert_eq!(stamp.take(), Some(version(10, LAST_SEQUENCE)?));
    assert!(stamp.is_full());
    assert_eq!(stamp.take(), None);
    Ok(())
}

#[test]
fn test_a_stamp_resumes_after_its_last_version_in_the_open_epoch() -> TestResult {
    let mut stamp = EpochStamp::after(epoch(9)?, version(10, 4)?);
    assert_eq!(stamp.take(), Some(version(10, 6)?));
    Ok(())
}

#[test]
fn test_a_last_version_of_a_closed_epoch_opens_the_next_at_0() -> TestResult {
    let mut stamp = EpochStamp::after(epoch(9)?, version(9, 40)?);
    assert_eq!(stamp.take(), Some(version(10, 0)?));
    assert_eq!(stamp.last(), Some(version(10, 0)?));
    Ok(())
}

#[test]
fn test_a_last_version_past_the_open_epoch_stamps_nothing_below_it() -> TestResult {
    // A recovered stamp newer than its delivered closes allow: no version may follow it in an earlier epoch.
    let mut stamp = EpochStamp::after(epoch(9)?, version(12, 0)?);
    assert!(stamp.is_full());
    assert_eq!(stamp.take(), None);
    assert_eq!(stamp.last(), Some(version(12, 0)?));
    Ok(())
}

#[test]
fn test_a_close_opens_the_next_epoch_at_sequence_0() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(10)?);
    let _first: Option<Version> = stamp.take();
    let _second: Option<Version> = stamp.take();
    assert_eq!(stamp.deliver(epoch(11)?), Some(true));
    assert_eq!(stamp.delivered(), epoch(11)?);
    assert_eq!(stamp.take(), Some(version(12, 0)?));
    Ok(())
}

#[test]
fn test_a_close_delivered_already_changes_nothing() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(9)?);
    let _taken: Option<Version> = stamp.take();
    let before = stamp;
    assert_eq!(stamp.deliver(epoch(9)?), Some(false));
    assert_eq!(stamp.deliver(epoch(5)?), Some(false));
    assert_eq!(stamp.deliver(Epoch::ZERO), Some(false));
    assert_eq!(stamp, before);
    assert_eq!(stamp.take(), Some(version(10, 2)?));
    Ok(())
}

#[test]
fn test_a_close_past_a_gap_is_refused() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(9)?);
    let before = stamp;
    assert_eq!(stamp.deliver(epoch(11)?), None);
    assert_eq!(stamp.deliver(Epoch::MAX), None);
    assert_eq!(stamp, before);
    assert_eq!(stamp.take(), Some(version(10, 0)?));
    Ok(())
}

#[test]
fn test_closes_delivered_in_order_raise_the_epoch_one_at_a_time() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(3)?);
    for e in 4..=9 {
        assert_eq!(stamp.deliver(epoch(e)?), Some(true), "close {e}");
    }
    assert_eq!(stamp.take(), Some(version(10, 0)?));
    Ok(())
}

#[test]
fn test_versions_strictly_increase_across_closes() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(1)?);
    let mut versions = Vec::new();
    for e in 2..=4 {
        versions.extend(stamp.take());
        versions.extend(stamp.take());
        assert_eq!(stamp.deliver(epoch(e)?), Some(true));
    }
    assert_eq!(versions.len(), 6);
    assert!(versions.windows(2).all(|w| w[0] < w[1]));
    Ok(())
}

#[test]
fn test_the_last_epoch_is_stamped_and_none_follows_it() -> TestResult {
    let below = Epoch::new(Epoch::MAX.get() - 1).context("the epoch")?;
    let mut stamp = EpochStamp::new(below);
    assert_eq!(stamp.open(), Some(Epoch::MAX));
    // (2^42 − 1) << 22 = 2^64 − 2^22 = 18,446,744,073,705,357,312.
    assert_eq!(
        stamp.take().map(Version::to_u64),
        Some(18_446_744_073_705_357_312)
    );
    assert_eq!(stamp.deliver(Epoch::MAX), Some(true));
    assert_eq!(stamp.open(), None);
    assert_eq!(stamp.take(), None);
    Ok(())
}

#[test]
fn test_a_peek_names_the_next_version_and_takes_nothing() -> TestResult {
    let mut stamp = EpochStamp::new(epoch(9)?);
    assert_eq!(stamp.peek(), Some(version(10, 0)?));
    assert_eq!(stamp.peek(), Some(version(10, 0)?));
    assert_eq!(stamp.last(), None);
    let _taken: Option<Version> = stamp.take();
    assert_eq!(stamp.peek(), Some(version(10, 2)?));
    let full = EpochStamp::after(epoch(9)?, version(10, LAST_SEQUENCE)?);
    assert_eq!(full.peek(), None);
    Ok(())
}
