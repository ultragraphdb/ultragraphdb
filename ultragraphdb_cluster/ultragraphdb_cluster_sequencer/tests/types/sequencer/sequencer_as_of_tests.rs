/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_cluster_sequencer::ClockReading;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::harness::{Log, block_on, config, fresh};

/// A sequencer whose epochs 1 to 5 closed at wall times 100, 1,000, 1,000, 2,000 and 3,000 µs, every millisecond.
fn closed() -> TestResult<ultragraphdb_cluster_sequencer::Sequencer<Log>> {
    let (_, mut sequencer) = fresh(config(1, 1)?)?;
    for (ms, wall) in [(1, 100), (2, 1_000), (3, 1_000), (4, 2_000), (5, 3_000)] {
        let _closed = sequencer.tick(ClockReading::new(ms * 1_000, wall), &[], 0, &[])?;
    }
    let _durable = block_on(sequencer.sync())??;
    Ok(sequencer)
}

fn as_of(
    sequencer: &ultragraphdb_cluster_sequencer::Sequencer<Log>,
    time_us: u64,
) -> TestResult<u64> {
    Ok(sequencer.as_of(time_us)?.get())
}

#[test]
fn test_a_time_maps_to_the_latest_close_at_or_before_it() -> TestResult {
    let sequencer = closed()?;
    assert_eq!(as_of(&sequencer, 100)?, 1);
    assert_eq!(as_of(&sequencer, 999)?, 1);
    // Epochs 2 and 3 both closed at 1,000: the later.
    assert_eq!(as_of(&sequencer, 1_000)?, 3);
    assert_eq!(as_of(&sequencer, 1_999)?, 3);
    assert_eq!(as_of(&sequencer, 2_000)?, 4);
    assert_eq!(as_of(&sequencer, 3_000)?, 5);
    assert_eq!(as_of(&sequencer, u64::MAX)?, 5);
    Ok(())
}

#[test]
fn test_a_time_before_the_first_close_is_refused() -> TestResult {
    let sequencer = closed()?;
    for time_us in [0, 99] {
        let refusal = sequencer.as_of(time_us).err().context("a refusal")?;
        assert_eq!(
            refusal.0,
            ClusterErrorEnum::BeforeFirstClose {
                time_us,
                first_us: Some(100)
            }
        );
    }
    Ok(())
}

#[test]
fn test_no_time_maps_before_any_close() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 1)?)?;
    let refusal = sequencer.as_of(5).err().context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::BeforeFirstClose {
            time_us: 5,
            first_us: None
        }
    );
    // A close not yet durable maps nothing either.
    let _closed = sequencer.tick(ClockReading::new(1_000, 1), &[], 0, &[])?;
    let unsynced = sequencer.as_of(5).err().context("a refusal")?;
    assert_eq!(
        unsynced.0,
        ClusterErrorEnum::BeforeFirstClose {
            time_us: 5,
            first_us: None
        }
    );
    Ok(())
}

#[test]
fn test_a_close_not_yet_durable_is_not_mapped() -> TestResult {
    let mut sequencer = closed()?;
    let _closed = sequencer.tick(ClockReading::new(6_000, 4_000), &[], 0, &[])?;
    assert_eq!(as_of(&sequencer, 4_000)?, 5);
    let _durable = block_on(sequencer.sync())??;
    assert_eq!(as_of(&sequencer, 4_000)?, 6);
    Ok(())
}

#[test]
fn test_the_oldest_close_held_bounds_the_mapping() -> TestResult {
    let two = NonZeroU32::new(2).context("2")?;
    let (_, mut sequencer) = fresh(config(1, 1)?.with_closes_retained(two))?;
    for (ms, wall) in [(1, 100), (2, 200), (3, 300)] {
        let _closed = sequencer.tick(ClockReading::new(ms * 1_000, wall), &[], 0, &[])?;
        let _durable = block_on(sequencer.sync())??;
    }
    assert_eq!(sequencer.as_of(250)?, Epoch::new(2)?);
    let refusal = sequencer.as_of(150).err().context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::BeforeFirstClose {
            time_us: 150,
            first_us: Some(200)
        }
    );
    Ok(())
}
