/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_cluster_sequencer::{Close, SequencedTransaction, SequencerRecord};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::harness::{
    at, block_on, close_through, config, epoch, fresh, pid, recover, write_log,
};

/// The epoch and body of each durable transaction from `from` on.
fn read(
    sequencer: &crate::utils::harness::SequencerOnLog,
    from: u64,
) -> TestResult<Vec<(u64, Vec<u8>)>> {
    Ok(sequencer
        .transactions_from(epoch(from)?)
        .iter()
        .map(|t| (t.epoch().get(), t.body().to_vec()))
        .collect())
}

#[test]
fn test_a_transaction_is_placed_in_the_open_epoch() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let (placed, position) = sequencer.submit(epoch(1)?, &[pid(1)?, pid(2)?], b"a")?;
    assert_eq!((placed.get(), position.get()), (1, 1));
    let _close = sequencer.tick(at(10), &[], &[])?;
    // A transaction after a close not yet durable is in the epoch after it.
    let (later, later_position) = sequencer.submit(epoch(1)?, &[pid(2)?], b"b")?;
    assert_eq!((later.get(), later_position.get()), (2, 3));
    let _durable = block_on(sequencer.sync())??;
    assert_eq!(
        read(&sequencer, 0)?,
        vec![(1, b"a".to_vec()), (2, b"b".to_vec())]
    );
    let first = *sequencer
        .transactions_from(epoch(0)?)
        .first()
        .context("the first")?;
    assert_eq!(first.partitions(), [pid(1)?, pid(2)?]);
    assert_eq!(first.reply_epoch().get(), 1);
    Ok(())
}

#[test]
fn test_a_transaction_is_read_back_only_once_durable() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let _placed = sequencer.submit(epoch(1)?, &[pid(1)?], b"a")?;
    assert!(sequencer.transactions_from(epoch(0)?).is_empty());
    let durable = block_on(sequencer.sync())??;
    assert_eq!(durable.get(), 1);
    assert_eq!(read(&sequencer, 0)?.len(), 1);
    Ok(())
}

#[test]
fn test_reading_from_an_epoch_skips_the_earlier_ones() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 1)?)?;
    for ms in 1..=4 {
        let _placed = sequencer.submit(epoch(1)?, &[pid(1)?], &[u8::try_from(ms)?])?;
        let _close = sequencer.tick(at(ms), &[], &[])?;
    }
    let _durable = block_on(sequencer.sync())??;
    // Transactions in epochs 1 to 4, one each.
    assert_eq!(read(&sequencer, 3)?, vec![(3, vec![3]), (4, vec![4])]);
    assert_eq!(read(&sequencer, 5)?, vec![]);
    assert_eq!(read(&sequencer, 1)?.len(), 4);
    Ok(())
}

#[test]
fn test_a_reply_epoch_equal_to_the_open_epoch_is_taken() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 1)?)?;
    let _ms = close_through(&mut sequencer, 12)?;
    let (placed, _) = sequencer.submit(epoch(13)?, &[pid(1)?], b"")?;
    assert_eq!(placed.get(), 13);
    Ok(())
}

#[test]
fn test_a_reply_epoch_past_the_open_epoch_is_refused() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 1)?)?;
    let _ms = close_through(&mut sequencer, 12)?;
    let refusal = sequencer
        .submit(epoch(14)?, &[pid(1)?], b"")
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::EpochAhead {
            epoch: 14,
            latest: 13
        }
    );
    let _durable = block_on(sequencer.sync())??;
    assert!(sequencer.transactions_from(epoch(0)?).is_empty());
    Ok(())
}

#[test]
fn test_a_refused_transaction_appends_nothing() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let refusal = sequencer
        .submit(epoch(1)?, &[], b"x")
        .err()
        .context("a refusal")?;
    assert_eq!(refusal.0, ClusterErrorEnum::NoPartitions);
    let duplicate = sequencer
        .submit(epoch(1)?, &[pid(2)?, pid(2)?], b"x")
        .err()
        .context("a refusal")?;
    assert_eq!(
        duplicate.0,
        ClusterErrorEnum::DuplicatePartition { partition: 2 }
    );
    // The next record takes position 1.
    let (_, position) = sequencer.submit(epoch(1)?, &[pid(2)?], b"x")?;
    assert_eq!(position.get(), 1);
    Ok(())
}

#[test]
fn test_the_sequencer_holds_at_most_its_retained_transactions() -> TestResult {
    let two = NonZeroU32::new(2).context("2")?;
    let (_, mut sequencer) = fresh(config(2, 10)?.with_transactions_retained(two))?;
    let _first = sequencer.submit(epoch(1)?, &[pid(1)?], b"1")?;
    let _second = sequencer.submit(epoch(1)?, &[pid(1)?], b"2")?;
    let refusal = sequencer
        .submit(epoch(1)?, &[pid(1)?], b"3")
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::Full {
            what: "multi-partition transactions",
            capacity: 2
        }
    );
    Ok(())
}

#[test]
fn test_no_transaction_is_placed_after_the_last_epoch() -> TestResult {
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
    let refusal = sequencer
        .submit(epoch(1)?, &[pid(1)?], b"")
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
fn test_a_transaction_too_large_is_refused() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    let body = vec![0; SequencedTransaction::BODY_BYTES_MAX + 1];
    let refusal = sequencer
        .submit(epoch(1)?, &[pid(1)?], &body)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::TooMany {
            what: "bytes of a transaction's body",
            count: 65_537,
            max: 65_536
        }
    );
    Ok(())
}
