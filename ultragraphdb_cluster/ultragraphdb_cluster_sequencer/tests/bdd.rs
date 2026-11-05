/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The sequencer's scenarios, bound to `features/sequencer/`, over the write-ahead log on its simulated storage.

#[path = "utils/harness.rs"]
mod harness;
#[path = "utils/partition_model.rs"]
mod partition_model;

use std::fmt;

use cucumber::{World, then, when};
use ultragraphdb_cluster_sequencer::{
    CatalogVersion, ClockReading, Close, SequencerConfig, SequencerRecord,
};
use ultragraphdb_core_ids::{Epoch, Tenant, Version};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use crate::harness::{
    SequencerOnLog, at, block_on, close_through, config, crash_and_recover, drain, epoch, fresh,
    pid, recover, write_log,
};
use crate::partition_model::PartitionModel;

#[derive(Default, World)]
struct Steps {
    storage: Option<SimStorage>,
    sequencer: Option<SequencerOnLog>,
    refusals: Vec<ClusterError>,
    closes: Vec<Close>,
    /// The monotonic time in milliseconds of the close a scenario watches, and of the close before it.
    close_ms: Option<(u64, u64)>,
    partitions: Vec<PartitionModel>,
    stamps: Vec<u64>,
    mapped: Option<Result<Epoch, ClusterError>>,
    pending: Vec<CatalogVersion>,
    placed: Vec<u64>,
    waiting_read: Option<Epoch>,
}

impl fmt::Debug for Steps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Steps")
            .field("refusals", &self.refusals)
            .field("closes", &self.closes)
            .field("stamps", &self.stamps)
            .finish_non_exhaustive()
    }
}

impl Steps {
    fn sequencer(&mut self) -> TestResult<&mut SequencerOnLog> {
        self.sequencer.as_mut().context("the sequencer")
    }

    fn storage(&self) -> TestResult<&SimStorage> {
        self.storage.as_ref().context("the storage")
    }

    fn start(&mut self, config: SequencerConfig) -> TestResult {
        let (storage, sequencer) = fresh(config)?;
        self.storage = Some(storage);
        self.sequencer = Some(sequencer);
        Ok(())
    }
}

/// The version `version` of tenant 1's catalog.
fn version(version: u64) -> CatalogVersion {
    CatalogVersion::new(Tenant::new(1), version)
}

#[when(
    "the sequencer runs for 100 ms with `epoch_ms_min` 5 and `epoch_ms_max` 10 on a stepped clock"
)]
fn runs_100_ms(w: &mut Steps) -> TestResult {
    w.start(config(5, 10)?)?;
    let sequencer = w.sequencer()?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    for ms in 0..=100 {
        let _closed: Option<Epoch> = sequencer.tick(at(ms), &[], 0, &[])?;
        let _durable = block_on(sequencer.sync())??;
    }
    w.closes = drain(w.sequencer()?, pid(1)?)?;
    Ok(())
}

#[then("it closes between 10 and 20 epochs, numbered consecutively from 1")]
fn closes_10_to_20(w: &mut Steps) -> TestResult {
    let epochs: Vec<u64> = w.closes.iter().map(|c| c.epoch().get()).collect();
    assert!((10..=20).contains(&epochs.len()), "{epochs:?}");
    let expected: Vec<u64> = (1..=u64::try_from(epochs.len())?).collect();
    assert_eq!(epochs, expected);
    Ok(())
}

#[when(
    "the frontend starts once with `epoch_ms_min` 0, once with `epoch_ms_min` 1,001, once with `epoch_ms_min` 5 and `epoch_ms_max` 4, and once with `epoch_ms_max` 10,001"
)]
fn starts_out_of_bounds(w: &mut Steps) -> TestResult {
    // Each start states both settings: the other one within its bounds.
    for (min, max) in [(0, 10), (1_001, 2_000), (5, 4), (5, 10_001)] {
        let refusal = SequencerConfig::new(min, max).err().context("a refusal")?;
        w.refusals.push(refusal);
    }
    Ok(())
}

#[then(
    "each start exits with code 78 and an error naming the setting and its bound: 1, 1,000, `epoch_ms_min` and 10,000"
)]
fn refused_naming_bounds(w: &mut Steps) -> TestResult {
    let refusals: Vec<_> = w.refusals.iter().map(|r| r.0.clone()).collect();
    assert_eq!(
        refusals,
        vec![
            ClusterErrorEnum::SettingBelow {
                setting: "epoch_ms_min",
                value: 0,
                bound: "its minimum",
                minimum: 1
            },
            ClusterErrorEnum::SettingAbove {
                setting: "epoch_ms_min",
                value: 1_001,
                maximum: 1_000
            },
            ClusterErrorEnum::SettingBelow {
                setting: "epoch_ms_max",
                value: 4,
                bound: "epoch_ms_min",
                minimum: 5
            },
            ClusterErrorEnum::SettingAbove {
                setting: "epoch_ms_max",
                value: 10_001,
                maximum: 10_000
            },
        ]
    );
    // The exit code 78 is the role's mapping of a configuration error (group 11); the texts name setting and bound.
    for (refusal, (setting, bound)) in w.refusals.iter().zip([
        ("epoch_ms_min", "1"),
        ("epoch_ms_min", "1000"),
        ("epoch_ms_max", "epoch_ms_min"),
        ("epoch_ms_max", "10000"),
    ]) {
        let text = refusal.to_string();
        assert!(text.starts_with(setting), "{text}");
        assert!(
            text.ends_with(bound) || text.contains(&format!("{bound},")),
            "{text}"
        );
    }
    Ok(())
}

/// Starts a sequencer with `epoch_ms_min` 2 and `epoch_ms_max` 10, closes epochs 1 to 6 and moves the clock to 1 ms
/// after the close of 6; returns that close's time in milliseconds.
fn epoch_6_closed_1_ms_ago(w: &mut Steps) -> TestResult<u64> {
    w.start(config(2, 10)?)?;
    let closed_ms = close_through(w.sequencer()?, 6)?;
    assert_eq!(w.sequencer()?.tick(at(closed_ms + 1), &[], 0, &[])?, None);
    Ok(closed_ms)
}

/// Ticks every millisecond from 1 ms after `since_ms` until epoch 7 closes; records its time and `since_ms`.
fn tick_until_7_closes(w: &mut Steps, since_ms: u64) -> TestResult {
    for ms in since_ms + 1..=since_ms + 10 {
        if w.sequencer()?.tick(at(ms), &[], 0, &[])? == Some(epoch(7)?) {
            w.close_ms = Some((ms, since_ms));
            return Ok(());
        }
    }
    Err(TestError::new("epoch 7 did not close within epoch_ms_max"))
}

#[when(
    "`epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a partition asks for a close"
)]
fn partition_asks(w: &mut Steps) -> TestResult {
    let closed_ms = epoch_6_closed_1_ms_ago(w)?;
    // The partition's open epoch is 7, one above the close of 6 delivered to it.
    w.sequencer()?.request_close(epoch(7)?)?;
    tick_until_7_closes(w, closed_ms)
}

#[then("the sequencer closes epoch 7 once 2 ms have passed since epoch 6 closed, not at 10 ms")]
fn closes_7_after_2_ms(w: &mut Steps) -> TestResult {
    let (closed_ms, since_ms) = w.close_ms.context("the close of 7")?;
    assert_eq!(closed_ms - since_ms, 2);
    Ok(())
}

#[when(
    "`epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a snapshot read waits on the open epoch 7"
)]
fn read_waits(w: &mut Steps) -> TestResult {
    let closed_ms = epoch_6_closed_1_ms_ago(w)?;
    w.close_ms = Some((0, closed_ms));
    w.waiting_read = Some(epoch(7)?);
    Ok(())
}

#[then(
    "the frontend asks its sequencer for a close, and epoch 7 closes once 2 ms have passed since epoch 6 closed"
)]
fn frontend_asks(w: &mut Steps) -> TestResult {
    let (_, since_ms) = w.close_ms.context("the close of 6")?;
    let waiting = w.waiting_read.context("the waiting read")?;
    w.sequencer()?.request_close(waiting)?;
    tick_until_7_closes(w, since_ms)?;
    let (closed_ms, _) = w.close_ms.context("the close of 7")?;
    assert_eq!(closed_ms - since_ms, 2);
    Ok(())
}

#[when("the clock steps back by one second between two closes")]
fn clock_steps_back(w: &mut Steps) -> TestResult {
    w.start(config(2, 10)?)?;
    let sequencer = w.sequencer()?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    let wall = 1_700_000_000_000_000;
    let _first = sequencer.tick(ClockReading::new(10_000, wall), &[], 0, &[])?;
    let _second = sequencer.tick(ClockReading::new(20_000, wall - 1_000_000), &[], 0, &[])?;
    let _durable = block_on(sequencer.sync())??;
    w.closes = drain(w.sequencer()?, pid(1)?)?;
    Ok(())
}

#[then("the later close's recorded time equals the earlier close's time, not one second less")]
fn later_time_equals_earlier(w: &mut Steps) -> TestResult {
    let times: Vec<u64> = w.closes.iter().map(Close::time_us).collect();
    assert_eq!(times, vec![1_700_000_000_000_000, 1_700_000_000_000_000]);
    Ok(())
}

#[when("the sequencer's last closed epoch is 2^42 − 1 and it is asked to close the next")]
fn last_epoch_asked(w: &mut Steps) -> TestResult {
    let storage = SimStorage::new(5);
    write_log(
        &storage,
        &[SequencerRecord::Close(Close::new(
            Epoch::MAX,
            0,
            vec![],
            vec![],
        )?)],
    )?;
    let mut sequencer = recover(&storage, config(2, 10)?, at(0))?;
    assert_eq!(sequencer.last_closed().get(), (1 << 42) - 1);
    sequencer.request_close(Epoch::MAX)?;
    let refusal = sequencer
        .tick(at(10), &[], 0, &[])
        .err()
        .context("a refusal")?;
    w.refusals.push(refusal);
    Ok(())
}

#[then("the close is refused with an error naming the 42-bit limit")]
fn refused_naming_42_bits(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::EpochLimit {
            epoch: 4_398_046_511_103
        }
    );
    assert!(refusal.to_string().contains("42-bit"), "{refusal}");
    Ok(())
}

/// Closes epochs every millisecond at the wall-clock times `walls`, in µs, synced.
fn close_at_walls(w: &mut Steps, walls: &[u64]) -> TestResult {
    w.start(config(1, 1)?)?;
    let sequencer = w.sequencer()?;
    for (ms, wall) in (1..).zip(walls) {
        let _closed = sequencer.tick(ClockReading::new(ms * 1_000, *wall), &[], 0, &[])?;
    }
    let _durable = block_on(sequencer.sync())??;
    Ok(())
}

#[when("epoch 4 closed at 1,000 µs and epoch 5 at 2,000 µs, and the time 1,500 µs is mapped")]
fn time_between_closes(w: &mut Steps) -> TestResult {
    close_at_walls(w, &[100, 200, 300, 1_000, 2_000])?;
    w.mapped = Some(w.sequencer()?.as_of(1_500));
    Ok(())
}

#[then("the result is epoch 4")]
fn result_is_4(w: &mut Steps) -> TestResult {
    let mapped = w.mapped.clone().context("the mapping")??;
    assert_eq!(mapped.get(), 4);
    Ok(())
}

#[when("the first close was at 1,000 µs and the time 500 µs is mapped")]
fn time_before_first(w: &mut Steps) -> TestResult {
    close_at_walls(w, &[1_000, 2_000])?;
    w.mapped = Some(w.sequencer()?.as_of(500));
    Ok(())
}

#[then("the mapping fails with an error naming 500 µs and 1,000 µs")]
fn mapping_fails(w: &mut Steps) -> TestResult {
    let refusal = w
        .mapped
        .clone()
        .context("the mapping")?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::BeforeFirstClose {
            time_us: 500,
            first_us: Some(1_000)
        }
    );
    let text = refusal.to_string();
    assert!(
        text.contains("500 µs") && text.contains("1000 µs"),
        "{text}"
    );
    Ok(())
}

/// Starts a sequencer with one subscribed partition, closes epochs 1 to `through` and delivers them to its model.
fn delivered_through(w: &mut Steps, through: u64) -> TestResult {
    w.start(config(1, 1)?)?;
    w.sequencer()?.subscribe(pid(1)?, epoch(0)?)?;
    let _ms = close_through(w.sequencer()?, through)?;
    deliver(w, 0)
}

/// Delivers every durable close not yet delivered to the partition of model `index`.
fn deliver(w: &mut Steps, index: usize) -> TestResult {
    let partition = pid(u32::try_from(index)? + 1)?;
    let closes = drain(w.sequencer()?, partition)?;
    if w.partitions.len() <= index {
        w.partitions.resize_with(index + 1, PartitionModel::default);
    }
    let model = w.partitions.get_mut(index).context("the model")?;
    closes.iter().try_for_each(|close| model.receive(close))
}

/// The sequencer closes the epoch after its last, synced, without delivering it.
fn close_next(w: &mut Steps, catalog: &[CatalogVersion]) -> TestResult<Epoch> {
    let sequencer = w.sequencer()?;
    let now = sequencer.deadline();
    let closed = sequencer
        .tick(ClockReading::new(now, now), catalog, 0, &[])?
        .context("a close")?;
    let _durable = block_on(sequencer.sync())??;
    Ok(closed)
}

#[when("epochs 1 to 7 are delivered to a partition and it commits a transaction")]
fn delivered_1_to_7_commits(w: &mut Steps) -> TestResult {
    delivered_through(w, 7)?;
    let model = w.partitions.first_mut().context("the partition")?;
    assert_eq!(model.delivered(), 7);
    w.stamps.push(model.commit_round());
    Ok(())
}

#[then("the commit's version carries epoch 8")]
fn version_carries_8(w: &mut Steps) -> TestResult {
    let stamped = *w.stamps.first().context("the commit")?;
    let version = Version::commit(Epoch::new(stamped)?, 0)?;
    assert_eq!(version.epoch().get(), 8);
    Ok(())
}

#[when(
    "epochs 1 to 7 are delivered to a partition, the sequencer has closed epoch 8, and its close has not yet reached the partition"
)]
fn close_8_not_delivered(w: &mut Steps) -> TestResult {
    delivered_through(w, 7)?;
    assert_eq!(close_next(w, &[])?.get(), 8);
    assert_eq!(w.sequencer()?.last_closed().get(), 8);
    Ok(())
}

#[then(
    "the partition's commits still carry epoch 8, and its first commit after the close of 8 arrives carries epoch 9"
)]
fn commits_carry_8_then_9(w: &mut Steps) -> TestResult {
    let before = w.partitions.first().context("the partition")?.stamp();
    assert_eq!(before, 8);
    deliver(w, 0)?;
    let after = w.partitions.first().context("the partition")?.stamp();
    assert_eq!(after, 9);
    Ok(())
}

#[when("a partition holds an unapplied round of epoch 8 and the close of epoch 8 is delivered")]
fn unapplied_round_of_8(w: &mut Steps) -> TestResult {
    delivered_through(w, 7)?;
    let round = w
        .partitions
        .first_mut()
        .context("the partition")?
        .commit_round();
    assert_eq!(round, 8);
    assert_eq!(close_next(w, &[])?.get(), 8);
    deliver(w, 0)
}

#[then(
    "the partition reports epoch 8 closed only after that round is applied, and a snapshot read at epoch 8 waits until then"
)]
fn closed_after_applied(w: &mut Steps) -> TestResult {
    let model = w.partitions.first_mut().context("the partition")?;
    assert_eq!(model.delivered(), 8);
    // A snapshot read at epoch 8 waits while the partition has closed only 7.
    assert_eq!(model.closed(), 7);
    model.apply_rounds();
    assert_eq!(model.closed(), 8);
    Ok(())
}

#[when("a partition loses its connection after epoch 3 and reconnects after epoch 9 closed")]
fn reconnects_after_9(w: &mut Steps) -> TestResult {
    delivered_through(w, 3)?;
    w.sequencer()?.unsubscribe(pid(1)?);
    let _ms = close_through(w.sequencer()?, 9)?;
    let after = epoch(w.partitions.first().context("the partition")?.delivered())?;
    w.sequencer()?.subscribe(pid(1)?, after)?;
    // A later close is made before the partition takes its deliveries.
    assert_eq!(close_next(w, &[])?.get(), 10);
    w.closes = drain(w.sequencer()?, pid(1)?)?;
    Ok(())
}

#[then("it receives epochs 4 to 9 in order before any later close")]
fn receives_4_to_9(w: &mut Steps) -> TestResult {
    let epochs: Vec<u64> = w.closes.iter().map(|c| c.epoch().get()).collect();
    assert_eq!(epochs, vec![4, 5, 6, 7, 8, 9, 10]);
    let model = w.partitions.first_mut().context("the partition")?;
    w.closes.iter().try_for_each(|close| model.receive(close))
}

#[when(
    "a catalog version is pending while epoch 38 is the latest close, and the sequencer closes epoch 39"
)]
fn pending_then_39(w: &mut Steps) -> TestResult {
    w.start(config(1, 1)?)?;
    w.sequencer()?.subscribe(pid(1)?, epoch(0)?)?;
    w.sequencer()?.subscribe(pid(2)?, epoch(0)?)?;
    let _ms = close_through(w.sequencer()?, 38)?;
    deliver(w, 0)?;
    deliver(w, 1)?;
    w.pending = vec![version(3)];
    for model in &w.partitions {
        // Before the close of 39 reaches it, a partition stamps 39.
        assert_eq!(model.stamp(), 39);
    }
    let pending = w.pending.clone();
    assert_eq!(close_next(w, &pending)?.get(), 39);
    Ok(())
}

#[then(
    "the close of epoch 39 assigns it effective epoch 40, its durable record holds the assignment, and every partition holds the version before it stamps a commit at epoch 40"
)]
fn assigned_40_everywhere(w: &mut Steps) -> TestResult {
    let pending = *w.pending.first().context("the version")?;
    // The durable record: the close of 39 as a restarted sequencer reads it back from the log.
    let storage = w.storage()?.clone();
    let mut restarted = crash_and_recover(&storage, config(1, 1)?, at(1_000))?;
    restarted.subscribe(pid(9)?, epoch(38)?)?;
    let close = restarted
        .take_delivery(pid(9)?)?
        .context("the close of 39")?;
    assert_eq!(close.epoch().get(), 39);
    assert_eq!(close.catalog(), [pending]);
    assert_eq!(close.effective_epoch().map(Epoch::get), Some(40));
    deliver(w, 0)?;
    deliver(w, 1)?;
    for model in &w.partitions {
        assert_eq!(model.stamp(), 40);
        assert!(model.holds(pending, 40));
    }
    Ok(())
}

#[when(
    "placement versions 1 and 2 are pending while epoch 38 is the latest close, and the sequencer closes epochs 39 and 40"
)]
fn placement_pending_then_39_40(w: &mut Steps) -> TestResult {
    w.start(config(1, 1)?)?;
    let _ms = close_through(w.sequencer()?, 38)?;
    // The host hands each close the oldest placement version pending: 1, and once the close of 39 is learned, 2.
    for (version, closed) in [(1, 39), (2, 40)] {
        let sequencer = w.sequencer()?;
        let now = sequencer.deadline();
        let epoch = sequencer
            .tick(ClockReading::new(now, now), &[], version, &[])?
            .context("a close")?;
        let _durable = block_on(sequencer.sync())??;
        assert_eq!(epoch.get(), closed);
    }
    Ok(())
}

#[then(
    "the close of 39 assigns version 1 effective epoch 40, and the close of 40 assigns version 2 effective epoch 41"
)]
fn placement_assigned_40_41(w: &mut Steps) -> TestResult {
    // The durable records: the closes as a restarted sequencer reads them back from the log.
    let storage = w.storage()?.clone();
    let mut restarted = crash_and_recover(&storage, config(1, 1)?, at(1_000))?;
    restarted.subscribe(pid(9)?, epoch(38)?)?;
    let closes = drain(&mut restarted, pid(9)?)?;
    let assigned: Vec<(u64, u64, Option<u64>)> = closes
        .iter()
        .map(|c| {
            (
                c.epoch().get(),
                c.placement(),
                c.effective_epoch().map(Epoch::get),
            )
        })
        .collect();
    assert_eq!(assigned, [(39, 1, Some(40)), (40, 2, Some(41))]);
    Ok(())
}

#[when(
    "a catalog version is committed as pending while epoch 38 is the latest close, and the frontend is killed before it closes epoch 39"
)]
fn killed_before_39(w: &mut Steps) -> TestResult {
    w.start(config(1, 1)?)?;
    let _ms = close_through(w.sequencer()?, 38)?;
    // The metadata group holds the version pending; it is durable there (group 6).
    w.pending = vec![version(3)];
    // The close of 39 is appended with it and the process is killed before the append is durable.
    let pending = w.pending.clone();
    let sequencer = w.sequencer()?;
    let now = sequencer.deadline();
    let appended = sequencer.tick(ClockReading::new(now, now), &pending, 0, &[])?;
    assert_eq!(appended.map(Epoch::get), Some(39));
    let storage = w.storage()?.clone();
    w.sequencer = Some(crash_and_recover(&storage, config(1, 1)?, at(1_000))?);
    Ok(())
}

#[then(
    "after recovery the version is still pending, and the next close, of epoch 39, assigns it effective epoch 40"
)]
fn assigned_after_recovery(w: &mut Steps) -> TestResult {
    let sequencer = w.sequencer()?;
    assert_eq!(sequencer.last_closed().get(), 38);
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    let closes = drain(sequencer, pid(1)?)?;
    // No durable close announced it: it is still pending.
    assert!(closes.iter().all(|c| c.catalog().is_empty()));
    let pending = w.pending.clone();
    assert_eq!(close_next(w, &pending)?.get(), 39);
    let close = w
        .sequencer()?
        .take_delivery(pid(1)?)?
        .context("the close of 39")?;
    assert_eq!(close.catalog(), pending.as_slice());
    assert_eq!(close.effective_epoch().map(Epoch::get), Some(40));
    Ok(())
}

#[when(
    "transactions A and B are accepted in epoch 6 and C in epoch 7, and the log is read from epoch 6"
)]
fn a_b_c_accepted(w: &mut Steps) -> TestResult {
    w.start(config(1, 1)?)?;
    let _ms = close_through(w.sequencer()?, 5)?;
    let sequencer = w.sequencer()?;
    let (a, _) = sequencer.submit(epoch(5)?, &[pid(1)?, pid(2)?], b"A")?;
    let (b, _) = sequencer.submit(epoch(5)?, &[pid(2)?, pid(3)?], b"B")?;
    w.placed.extend([a.get(), b.get()]);
    assert_eq!(close_next(w, &[])?.get(), 6);
    let (c, _) = w
        .sequencer()?
        .submit(epoch(6)?, &[pid(1)?, pid(3)?], b"C")?;
    w.placed.push(c.get());
    let _durable = block_on(w.sequencer()?.sync())??;
    Ok(())
}

#[then("it returns A and B with epoch 6 and C with epoch 7, in that order")]
fn returns_a_b_c(w: &mut Steps) -> TestResult {
    assert_eq!(w.placed, vec![6, 6, 7]);
    let read: Vec<(Vec<u8>, u64)> = w
        .sequencer()?
        .transactions_from(epoch(6)?)
        .iter()
        .map(|t| (t.body().to_vec(), t.epoch().get()))
        .collect();
    assert_eq!(
        read,
        vec![(b"A".to_vec(), 6), (b"B".to_vec(), 6), (b"C".to_vec(), 7)]
    );
    Ok(())
}

#[when(
    "the latest close is 12 and a multi-partition transaction naming reply epoch 12 is accepted"
)]
fn reply_epoch_12(w: &mut Steps) -> TestResult {
    w.start(config(1, 1)?)?;
    let _ms = close_through(w.sequencer()?, 12)?;
    let (placed, _) = w
        .sequencer()?
        .submit(epoch(12)?, &[pid(1)?, pid(2)?], b"")?;
    w.placed.push(placed.get());
    let _durable = block_on(w.sequencer()?.sync())??;
    Ok(())
}

#[then("the log places it in epoch 13")]
fn placed_in_13(w: &mut Steps) -> TestResult {
    assert_eq!(w.placed, vec![13]);
    let read: Vec<u64> = w
        .sequencer()?
        .transactions_from(epoch(0)?)
        .iter()
        .map(|t| t.epoch().get())
        .collect();
    assert_eq!(read, vec![13]);
    Ok(())
}

#[when("a multi-partition transaction naming no partition is offered")]
fn no_partition_offered(w: &mut Steps) -> TestResult {
    w.start(config(1, 1)?)?;
    let refusal = w
        .sequencer()?
        .submit(epoch(0)?, &[], b"x")
        .err()
        .context("a refusal")?;
    w.refusals.push(refusal);
    Ok(())
}

#[then("it is refused with an error naming the empty partition list, and the log is unchanged")]
fn refused_empty_list(w: &mut Steps) -> TestResult {
    let refusal = w.refusals.first().context("the refusal")?;
    assert_eq!(refusal.0, ClusterErrorEnum::NoPartitions);
    assert!(
        refusal.to_string().contains("partition list is empty"),
        "{refusal}"
    );
    let sequencer = w.sequencer()?;
    assert_eq!(block_on(sequencer.sync())??.get(), 0);
    assert!(sequencer.transactions_from(epoch(0)?).is_empty());
    Ok(())
}

#[when(
    "the sequencer closes epochs 1 to 12, accepts three transactions, and is killed and restarted"
)]
fn closes_12_killed(w: &mut Steps) -> TestResult {
    w.start(config(1, 1)?)?;
    for (through, body) in [(4, b"1"), (8, b"2"), (12, b"3")] {
        let _ms = close_through(w.sequencer()?, through)?;
        let _placed = w
            .sequencer()?
            .submit(epoch(through)?, &[pid(1)?, pid(2)?], body)?;
    }
    let _durable = block_on(w.sequencer()?.sync())??;
    let storage = w.storage()?.clone();
    w.sequencer = Some(crash_and_recover(&storage, config(1, 1)?, at(1_000))?);
    Ok(())
}

#[then(
    "its last closed epoch is 12, and reading its log returns the three transactions in their order"
)]
fn recovered_12(w: &mut Steps) -> TestResult {
    let sequencer = w.sequencer()?;
    assert_eq!(sequencer.last_closed().get(), 12);
    let read: Vec<(Vec<u8>, u64)> = sequencer
        .transactions_from(epoch(0)?)
        .iter()
        .map(|t| (t.body().to_vec(), t.epoch().get()))
        .collect();
    assert_eq!(
        read,
        vec![(b"1".to_vec(), 5), (b"2".to_vec(), 9), (b"3".to_vec(), 13)]
    );
    Ok(())
}

#[when("the sequencer is killed after appending a close and before the append is durable")]
fn killed_before_durable(w: &mut Steps) -> TestResult {
    delivered_through(w, 3)?;
    let sequencer = w.sequencer()?;
    let now = sequencer.deadline();
    let appended = sequencer.tick(ClockReading::new(now, now), &[], 0, &[])?;
    assert_eq!(appended.map(Epoch::get), Some(4));
    // Before the crash, nothing of the close of 4 reaches the partition.
    w.closes = drain(sequencer, pid(1)?)?;
    let storage = w.storage()?.clone();
    w.sequencer = Some(crash_and_recover(&storage, config(1, 1)?, at(1_000))?);
    Ok(())
}

#[then(
    "no partition received that close before the crash, and after restart the close is either durable and delivered or absent"
)]
fn durable_or_absent(w: &mut Steps) -> TestResult {
    assert!(w.closes.is_empty(), "{:?}", w.closes);
    let after = epoch(w.partitions.first().context("the partition")?.delivered())?;
    let sequencer = w.sequencer()?;
    sequencer.subscribe(pid(1)?, after)?;
    let delivered: Vec<u64> = drain(sequencer, pid(1)?)?
        .iter()
        .map(|c| c.epoch().get())
        .collect();
    let last = sequencer.last_closed().get();
    assert!(
        (delivered.is_empty() && last == 3) || (delivered == [4] && last == 4),
        "{delivered:?}, last closed {last}"
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
