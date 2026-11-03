/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_cluster_sequencer::{
    CatalogVersion, ClockReading, Close, SequencedTransaction, Sequencer, SequencerRecord,
};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::{OrderedLog, RoundRef};
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::harness::{
    at, block_on, close_through, config, crash_and_recover, drain, epoch, fresh, open, pid,
    recover, write_log,
};

fn close(at_epoch: u64, time_us: u64) -> TestResult<SequencerRecord> {
    Ok(SequencerRecord::Close(Close::new(
        epoch(at_epoch)?,
        time_us,
        vec![],
        vec![],
    )?))
}

fn transaction(at_epoch: u64, body: &[u8]) -> TestResult<SequencerRecord> {
    Ok(SequencerRecord::Transaction(SequencedTransaction::new(
        epoch(at_epoch)?,
        epoch(at_epoch)?,
        vec![pid(1)?],
        body.to_vec(),
    )?))
}

/// The refusal of recovering from a log holding `records`.
fn refusal(records: &[SequencerRecord]) -> TestResult<ClusterErrorEnum> {
    let storage = SimStorage::new(11);
    write_log(&storage, records)?;
    let log = open(&storage)?;
    let recovered = block_on(Sequencer::recover(log, config(2, 10)?, at(0)))?;
    Ok(recovered.err().context("a refusal")?.0)
}

/// Appends one round holding `body`, its version `version`, to the log of `storage`.
fn append_raw(storage: &SimStorage, version: u64, body: &[u8]) -> TestResult {
    let mut log = open(storage)?;
    let (tail, parent) = log.tail();
    let position = tail.next().context("a next position")?;
    let _appended = log.append(RoundRef::new(
        position,
        parent,
        version,
        0,
        NonZeroU32::MIN,
        body,
    ))?;
    let _durable = block_on(log.sync())??;
    Ok(())
}

#[test]
fn test_recovery_rebuilds_the_closes_and_transactions() -> TestResult {
    let config = config(1, 2)?;
    let (storage, mut sequencer) = fresh(config)?;
    let _ms = close_through(&mut sequencer, 4)?;
    let _a = sequencer.submit(epoch(4)?, &[pid(1)?, pid(2)?], b"a")?;
    let pending = [CatalogVersion::new(Namespace::new(2), 9)];
    let _closed = sequencer.tick(
        ClockReading::new(sequencer.deadline(), 77_000),
        &pending,
        &[0xE1],
    )?;
    let _b = sequencer.submit(epoch(5)?, &[pid(3)?], b"b")?;
    let _durable = block_on(sequencer.sync())??;
    let mut recovered = crash_and_recover(&storage, config, at(100))?;
    assert_eq!(recovered.last_closed().get(), 5);
    let read: Vec<_> = recovered
        .transactions_from(epoch(0)?)
        .iter()
        .map(|t| (t.epoch().get(), t.partitions().to_vec(), t.body().to_vec()))
        .collect();
    assert_eq!(
        read,
        vec![
            (5, vec![pid(1)?, pid(2)?], b"a".to_vec()),
            (6, vec![pid(3)?], b"b".to_vec())
        ]
    );
    recovered.subscribe(pid(1)?, epoch(0)?)?;
    let closes = drain(&mut recovered, pid(1)?)?;
    assert_eq!(closes.len(), 5);
    let fifth = closes.last().context("the close of 5")?;
    assert_eq!(
        (fifth.time_us(), fifth.catalog(), fifth.evictions()),
        (77_000, &pending[..], &[0xE1][..])
    );
    assert_eq!(recovered.as_of(77_000)?.get(), 5);
    Ok(())
}

#[test]
fn test_a_recovered_sequencer_continues_the_epochs() -> TestResult {
    let config = config(2, 10)?;
    let (storage, mut sequencer) = fresh(config)?;
    let _ms = close_through(&mut sequencer, 3)?;
    let mut recovered = crash_and_recover(&storage, config, at(500))?;
    // The next close is due epoch_ms_max after the restart, and is epoch 4.
    assert_eq!(recovered.deadline(), 510_000);
    assert_eq!(recovered.tick(at(509), &[], &[])?, None);
    assert_eq!(recovered.tick(at(510), &[], &[])?.map(Epoch::get), Some(4));
    Ok(())
}

#[test]
fn test_a_close_time_never_falls_below_the_recovered_last() -> TestResult {
    let config = config(2, 10)?;
    let (storage, mut sequencer) = fresh(config)?;
    let _closed = sequencer.tick(ClockReading::new(10_000, 9_000_000), &[], &[])?;
    let _durable = block_on(sequencer.sync())??;
    let mut recovered = crash_and_recover(&storage, config, ClockReading::new(0, 1_000))?;
    let _closed_2 = recovered.tick(ClockReading::new(10_000, 2_000), &[], &[])?;
    let _durable_2 = block_on(recovered.sync())??;
    recovered.subscribe(pid(1)?, epoch(1)?)?;
    let second = recovered
        .take_delivery(pid(1)?)?
        .context("the close of 2")?;
    assert_eq!(second.time_us(), 9_000_000);
    Ok(())
}

#[test]
fn test_an_empty_log_starts_a_fresh_sequencer() -> TestResult {
    let storage = SimStorage::new(2);
    let sequencer = recover(&storage, config(2, 10)?, at(3))?;
    assert_eq!(sequencer.last_closed().get(), 0);
    assert_eq!(sequencer.deadline(), 13_000);
    assert!(sequencer.transactions_from(epoch(0)?).is_empty());
    Ok(())
}

#[test]
fn test_a_log_may_begin_at_any_close() -> TestResult {
    // A log whose earlier rounds were trimmed begins at a later close.
    let storage = SimStorage::new(2);
    write_log(
        &storage,
        &[close(40, 5)?, transaction(41, b"x")?, close(41, 6)?],
    )?;
    let sequencer = recover(&storage, config(2, 10)?, at(0))?;
    assert_eq!(sequencer.last_closed().get(), 41);
    assert_eq!(sequencer.transactions_from(epoch(41)?).len(), 1);
    Ok(())
}

#[test]
fn test_a_close_not_one_after_the_last_diverges() -> TestResult {
    assert_eq!(
        refusal(&[close(1, 0)?, close(3, 0)?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 2,
            what: "a close one after the last"
        }
    );
    assert_eq!(
        refusal(&[close(2, 0)?, close(2, 0)?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 2,
            what: "a close one after the last"
        }
    );
    assert_eq!(
        refusal(&[close(0, 0)?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 1,
            what: "a close one after the last"
        }
    );
    // A transaction first fixes the epoch: the close after it must close that epoch.
    assert_eq!(
        refusal(&[transaction(1, b"")?, close(2, 0)?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 2,
            what: "a close one after the last"
        }
    );
    Ok(())
}

#[test]
fn test_a_close_time_below_the_last_diverges() -> TestResult {
    assert_eq!(
        refusal(&[close(1, 500)?, close(2, 499)?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 2,
            what: "a close time at or above the last"
        }
    );
    Ok(())
}

#[test]
fn test_a_transaction_outside_the_open_epoch_diverges() -> TestResult {
    assert_eq!(
        refusal(&[close(1, 0)?, transaction(1, b"")?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 2,
            what: "a transaction in the epoch after the last close"
        }
    );
    assert_eq!(
        refusal(&[close(1, 0)?, transaction(3, b"")?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 2,
            what: "a transaction in the epoch after the last close"
        }
    );
    assert_eq!(
        refusal(&[transaction(2, b"")?])?,
        ClusterErrorEnum::ReplayDiverged {
            position: 1,
            what: "a transaction in the epoch after the last close"
        }
    );
    Ok(())
}

#[test]
fn test_a_rounds_version_other_than_its_epoch_diverges() -> TestResult {
    let storage = SimStorage::new(4);
    let mut body = Vec::new();
    close(1, 0)?.encode(&mut body);
    append_raw(&storage, 2, &body)?;
    let recovered = block_on(Sequencer::recover(open(&storage)?, config(2, 10)?, at(0)))?;
    assert_eq!(
        recovered.err().context("a refusal")?.0,
        ClusterErrorEnum::ReplayDiverged {
            position: 1,
            what: "a round's version equal to its record's epoch"
        }
    );
    Ok(())
}

#[test]
fn test_a_round_holding_no_record_is_malformed() -> TestResult {
    let storage = SimStorage::new(4);
    append_raw(&storage, 1, &[9, 9, 9])?;
    let recovered = block_on(Sequencer::recover(open(&storage)?, config(2, 10)?, at(0)))?;
    assert_eq!(
        recovered.err().context("a refusal")?.0,
        ClusterErrorEnum::Malformed {
            what: "a kind byte, 1 or 2",
            offset: 0
        }
    );
    Ok(())
}

#[test]
fn test_more_transactions_than_retained_refuse_the_recovery() -> TestResult {
    let storage = SimStorage::new(4);
    write_log(&storage, &[transaction(1, b"1")?, transaction(1, b"2")?])?;
    let one = NonZeroU32::MIN;
    let recovered = block_on(Sequencer::recover(
        open(&storage)?,
        config(2, 10)?.with_transactions_retained(one),
        at(0),
    ))?;
    assert_eq!(
        recovered.err().context("a refusal")?.0,
        ClusterErrorEnum::Full {
            what: "multi-partition transactions",
            capacity: 1
        }
    );
    Ok(())
}

#[test]
fn test_recovery_holds_only_the_retained_closes() -> TestResult {
    let config = config(1, 1)?.with_closes_retained(NonZeroU32::MIN.saturating_add(1));
    let storage = SimStorage::new(4);
    write_log(&storage, &[close(1, 10)?, close(2, 20)?, close(3, 30)?])?;
    let mut sequencer = recover(&storage, config, at(0))?;
    assert_eq!(sequencer.last_closed().get(), 3);
    assert_eq!(sequencer.as_of(20)?.get(), 2);
    let refusal = sequencer.as_of(10).err().context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::BeforeFirstClose {
            time_us: 10,
            first_us: Some(20)
        }
    );
    sequencer.subscribe(pid(1)?, epoch(1)?)?;
    let closes = drain(&mut sequencer, pid(1)?)?;
    assert_eq!(
        closes.iter().map(|c| c.epoch().get()).collect::<Vec<_>>(),
        vec![2, 3]
    );
    Ok(())
}
