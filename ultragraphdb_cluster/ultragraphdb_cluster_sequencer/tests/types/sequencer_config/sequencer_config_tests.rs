/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

fn refusal(min: u64, max: u64) -> TestResult<ClusterErrorEnum> {
    Ok(SequencerConfig::new(min, max).err().context("a refusal")?.0)
}

#[test]
fn test_the_bounds_and_the_defaults() {
    assert_eq!(SequencerConfig::EPOCH_MS_MIN_MAX, 1_000);
    assert_eq!(SequencerConfig::EPOCH_MS_MAX_MAX, 10_000);
    assert_eq!(SequencerConfig::CLOSES_RETAINED.get(), 65_536);
    assert_eq!(SequencerConfig::TRANSACTIONS_RETAINED.get(), 65_536);
}

#[test]
fn test_settings_at_their_bounds_are_taken() -> TestResult {
    for (min, max) in [
        (1, 1),
        (1, 10_000),
        (1_000, 1_000),
        (1_000, 10_000),
        (2, 10),
    ] {
        let config = SequencerConfig::new(min, max)?;
        assert_eq!((config.epoch_ms_min(), config.epoch_ms_max()), (min, max));
        assert_eq!(config.closes_retained(), SequencerConfig::CLOSES_RETAINED);
        assert_eq!(
            config.transactions_retained(),
            SequencerConfig::TRANSACTIONS_RETAINED
        );
    }
    Ok(())
}

#[test]
fn test_epoch_ms_min_below_one_is_refused() -> TestResult {
    assert_eq!(
        refusal(0, 10)?,
        ClusterErrorEnum::SettingBelow {
            setting: "epoch_ms_min",
            value: 0,
            bound: "its minimum",
            minimum: 1
        }
    );
    Ok(())
}

#[test]
fn test_epoch_ms_min_above_1000_is_refused() -> TestResult {
    assert_eq!(
        refusal(1_001, 2_000)?,
        ClusterErrorEnum::SettingAbove {
            setting: "epoch_ms_min",
            value: 1_001,
            maximum: 1_000
        }
    );
    Ok(())
}

#[test]
fn test_epoch_ms_max_below_epoch_ms_min_is_refused() -> TestResult {
    assert_eq!(
        refusal(5, 4)?,
        ClusterErrorEnum::SettingBelow {
            setting: "epoch_ms_max",
            value: 4,
            bound: "epoch_ms_min",
            minimum: 5
        }
    );
    Ok(())
}

#[test]
fn test_epoch_ms_max_above_10000_is_refused() -> TestResult {
    assert_eq!(
        refusal(5, 10_001)?,
        ClusterErrorEnum::SettingAbove {
            setting: "epoch_ms_max",
            value: 10_001,
            maximum: 10_000
        }
    );
    Ok(())
}

#[test]
fn test_epoch_ms_min_is_checked_first() -> TestResult {
    let ClusterErrorEnum::SettingBelow { setting, .. } = refusal(0, 0)? else {
        return Err(TestError::new("a setting below its bound"));
    };
    assert_eq!(setting, "epoch_ms_min");
    let ClusterErrorEnum::SettingAbove { setting: above, .. } = refusal(1_001, 0)? else {
        return Err(TestError::new("a above above its bound"));
    };
    assert_eq!(above, "epoch_ms_min");
    Ok(())
}

#[test]
fn test_the_retention_is_set() -> TestResult {
    let three = NonZeroU32::new(3).context("3")?;
    let five = NonZeroU32::new(5).context("5")?;
    let config = SequencerConfig::new(2, 10)?
        .with_closes_retained(three)
        .with_transactions_retained(five);
    assert_eq!(config.closes_retained(), three);
    assert_eq!(config.transactions_retained(), five);
    assert_eq!((config.epoch_ms_min(), config.epoch_ms_max()), (2, 10));
    Ok(())
}
