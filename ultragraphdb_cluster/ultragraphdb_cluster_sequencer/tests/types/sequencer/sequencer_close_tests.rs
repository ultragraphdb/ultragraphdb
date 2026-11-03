/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_sequencer::{CatalogVersion, ClockReading, Close, SequencerRecord};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::harness::{
    at, block_on, close_through, config, drain, epoch, fresh, pid, recover, write_log,
};

#[test]
fn test_no_close_before_the_deadline() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    assert_eq!(sequencer.tick(at(0), &[], 0, &[])?, None);
    assert_eq!(sequencer.tick(at(9), &[], 0, &[])?, None);
    assert_eq!(
        sequencer.tick(at(10), &[], 0, &[])?.map(Epoch::get),
        Some(1)
    );
    // The next is due 10 ms after the close, at 20.
    assert_eq!(sequencer.deadline(), 20_000);
    assert_eq!(sequencer.tick(at(19), &[], 0, &[])?, None);
    assert_eq!(
        sequencer.tick(at(20), &[], 0, &[])?.map(Epoch::get),
        Some(2)
    );
    Ok(())
}

#[test]
fn test_a_late_tick_closes_once_and_counts_from_itself() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    // Called at 35 ms, long after the deadline at 10: one close, the next due at 45.
    assert_eq!(
        sequencer.tick(at(35), &[], 0, &[])?.map(Epoch::get),
        Some(1)
    );
    assert_eq!(sequencer.tick(at(35), &[], 0, &[])?, None);
    assert_eq!(sequencer.deadline(), 45_000);
    Ok(())
}

#[test]
fn test_a_request_closes_once_epoch_ms_min_has_passed() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let _first = sequencer.tick(at(10), &[], 0, &[])?;
    // Epoch 2 is open; asked at 11 ms, it closes at 12, epoch_ms_min after the close of 1.
    sequencer.request_close(epoch(2)?)?;
    assert_eq!(sequencer.deadline(), 12_000);
    assert_eq!(sequencer.tick(at(11), &[], 0, &[])?, None);
    assert_eq!(
        sequencer.tick(at(12), &[], 0, &[])?.map(Epoch::get),
        Some(2)
    );
    // The request is spent: epoch 3 waits for epoch_ms_max.
    assert_eq!(sequencer.deadline(), 22_000);
    Ok(())
}

#[test]
fn test_a_request_after_epoch_ms_min_closes_at_once() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let _first = sequencer.tick(at(10), &[], 0, &[])?;
    sequencer.request_close(epoch(2)?)?;
    assert_eq!(
        sequencer.tick(at(15), &[], 0, &[])?.map(Epoch::get),
        Some(2)
    );
    Ok(())
}

#[test]
fn test_a_request_for_a_closed_epoch_changes_nothing() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let _first = sequencer.tick(at(10), &[], 0, &[])?;
    // Epoch 1 is closed, though not yet durable.
    sequencer.request_close(epoch(1)?)?;
    sequencer.request_close(epoch(0)?)?;
    assert_eq!(sequencer.deadline(), 20_000);
    Ok(())
}

#[test]
fn test_a_request_past_the_open_epoch_is_refused() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let refusal = sequencer
        .request_close(epoch(2)?)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::EpochAhead {
            epoch: 2,
            latest: 1
        }
    );
    assert_eq!(sequencer.deadline(), 10_000);
    Ok(())
}

#[test]
fn test_a_close_records_the_wall_clock_time() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    let _first = sequencer.tick(ClockReading::new(10_000, 5_000_123), &[], 0, &[])?;
    let _second = sequencer.tick(ClockReading::new(20_000, 5_010_456), &[], 0, &[])?;
    let _durable = block_on(sequencer.sync())??;
    let times: Vec<u64> = drain(&mut sequencer, pid(1)?)?
        .iter()
        .map(Close::time_us)
        .collect();
    assert_eq!(times, vec![5_000_123, 5_010_456]);
    Ok(())
}

#[test]
fn test_a_clock_stepping_back_records_the_last_closes_time() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    let _first = sequencer.tick(ClockReading::new(10_000, 3_000_000), &[], 0, &[])?;
    // One second back, then half a second back, then forward past the first.
    let _second = sequencer.tick(ClockReading::new(20_000, 2_000_000), &[], 0, &[])?;
    let _third = sequencer.tick(ClockReading::new(30_000, 2_500_000), &[], 0, &[])?;
    let _fourth = sequencer.tick(ClockReading::new(40_000, 3_000_001), &[], 0, &[])?;
    let _durable = block_on(sequencer.sync())??;
    let times: Vec<u64> = drain(&mut sequencer, pid(1)?)?
        .iter()
        .map(Close::time_us)
        .collect();
    assert_eq!(times, vec![3_000_000, 3_000_000, 3_000_000, 3_000_001]);
    Ok(())
}

#[test]
fn test_a_close_carries_the_pending_versions_and_evictions() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    let pending = [
        CatalogVersion::new(Namespace::new(4), 3),
        CatalogVersion::new(Namespace::new(5), 1),
    ];
    let _first = sequencer.tick(at(10), &pending, 0, &[0xA1, 0xB2])?;
    let _second = sequencer.tick(at(20), &[], 0, &[])?;
    let _durable = block_on(sequencer.sync())??;
    let closes = drain(&mut sequencer, pid(1)?)?;
    let first = closes.first().context("the close of 1")?;
    assert_eq!(first.catalog(), pending);
    assert_eq!(first.evictions(), [0xA1, 0xB2]);
    assert_eq!(first.effective_epoch().map(Epoch::get), Some(2));
    let second = closes.get(1).context("the close of 2")?;
    assert!(second.catalog().is_empty());
    assert!(second.evictions().is_empty());
    Ok(())
}

#[test]
fn test_too_many_versions_or_evictions_refuse_the_close() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let versions: Vec<_> = (0..257)
        .map(|v| CatalogVersion::new(Namespace::new(1), v))
        .collect();
    let refusal = sequencer
        .tick(at(10), &versions, 0, &[])
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::TooMany {
            what: "catalog versions in a close",
            count: 257,
            max: 256
        }
    );
    let evictions: Vec<u128> = (0..4_097).collect();
    let too_many_evictions = sequencer
        .tick(at(10), &[], 0, &evictions)
        .err()
        .context("a refusal")?;
    assert_eq!(
        too_many_evictions.0,
        ClusterErrorEnum::TooMany {
            what: "session evictions in a close",
            count: 4_097,
            max: 4_096
        }
    );
    // Nothing was appended: the next close is still epoch 1, still due.
    assert_eq!(
        sequencer.tick(at(10), &[], 0, &[])?.map(Epoch::get),
        Some(1)
    );
    Ok(())
}

#[test]
fn test_the_last_epoch_refuses_the_next_close() -> TestResult {
    let storage = SimStorage::new(5);
    let last = Close::new(Epoch::MAX, 1_000, vec![], vec![])?;
    write_log(&storage, &[SequencerRecord::Close(last)])?;
    let mut sequencer = recover(&storage, config(2, 10)?, at(0))?;
    assert_eq!(sequencer.last_closed(), Epoch::MAX);
    let refusal = sequencer
        .tick(at(10), &[], 0, &[])
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::EpochLimit {
            epoch: 4_398_046_511_103
        }
    );
    Ok(())
}

#[test]
fn test_closes_are_numbered_from_one_without_gaps() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 3)?)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    let _last_ms = close_through(&mut sequencer, 25)?;
    let epochs: Vec<u64> = drain(&mut sequencer, pid(1)?)?
        .iter()
        .map(|c| c.epoch().get())
        .collect();
    assert_eq!(epochs, (1..=25).collect::<Vec<u64>>());
    Ok(())
}

#[test]
fn test_a_close_records_the_placement_version_it_was_handed() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    // No pending placement version is 0; a tick not due records nothing, whatever it is handed.
    assert_eq!(sequencer.tick(at(0), &[], 7, &[])?, None);
    let _first = sequencer.tick(at(10), &[], 0, &[])?;
    let _second = sequencer.tick(at(20), &[], 3, &[])?;
    let _third = sequencer.tick(at(30), &[], u64::MAX, &[])?;
    let _durable = block_on(sequencer.sync())??;
    let placed: Vec<(u64, u64)> = drain(&mut sequencer, pid(1)?)?
        .iter()
        .map(|close| (close.epoch().get(), close.placement()))
        .collect();
    assert_eq!(placed, [(1, 0), (2, 3), (3, u64::MAX)]);
    Ok(())
}
