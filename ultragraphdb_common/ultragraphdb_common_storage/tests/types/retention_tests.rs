/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::Retention;
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_tool_bdd::TestResult;

/// The retention these tests hold, and its microseconds.
const RETAIN_MS: u64 = 60_000;
const RETAIN_US: u64 = RETAIN_MS * 1_000;

/// An epoch stays until it has been closed for more than the retention, not at it, and the bound asked is its own.
#[test]
fn test_an_epoch_goes_only_past_the_retention() -> TestResult {
    let mut retention = Retention::new(RETAIN_MS);
    let epoch = Epoch::new(20)?;
    retention.closed(epoch, 1_000);
    assert_eq!(
        retention.due(1_000 + RETAIN_US),
        None,
        "closed for exactly it"
    );
    assert_eq!(
        retention.due(1_001 + RETAIN_US),
        Some(Version::bound_of(epoch))
    );
    assert_eq!(retention.due(3_000_000 + RETAIN_US), None, "nothing new");
    Ok(())
}

/// One ask covers every epoch past the retention, by the latest; a later one stays.
#[test]
fn test_one_ask_takes_the_latest_epoch_past_it() -> TestResult {
    let mut retention = Retention::new(RETAIN_MS);
    for (epoch, at_us) in [(1, 0), (2, 10), (3, RETAIN_US)] {
        retention.closed(Epoch::new(epoch)?, at_us);
    }
    assert_eq!(
        retention.due(RETAIN_US + 11),
        Some(Version::bound_of(Epoch::new(2)?))
    );
    Ok(())
}

/// A raise waits out a second since the last, or the retention when shorter, each being a synced write.
#[test]
fn test_raises_are_spaced() -> TestResult {
    let mut retention = Retention::new(RETAIN_MS);
    retention.closed(Epoch::new(1)?, 0);
    let first_at = RETAIN_US + 1;
    assert!(retention.due(first_at).is_some());
    retention.closed(Epoch::new(2)?, 1);
    assert_eq!(retention.due(first_at + 1), None, "within the second");
    assert!(retention.due(first_at + 1_000_000).is_some());
    let mut short = Retention::new(10);
    short.closed(Epoch::new(1)?, 0);
    assert!(short.due(10_001).is_some());
    short.closed(Epoch::new(2)?, 10_001);
    assert_eq!(short.due(20_000), None, "within the retention");
    assert!(short.due(20_002).is_some());
    Ok(())
}

/// An epoch noted twice, or below the latest, is one close.
#[test]
fn test_an_epoch_noted_again_changes_nothing() -> TestResult {
    let mut retention = Retention::new(RETAIN_MS);
    let epoch = Epoch::new(5)?;
    retention.closed(epoch, 0);
    retention.closed(epoch, RETAIN_US);
    assert_eq!(retention.due(RETAIN_US + 1), Some(Version::bound_of(epoch)));
    Ok(())
}
