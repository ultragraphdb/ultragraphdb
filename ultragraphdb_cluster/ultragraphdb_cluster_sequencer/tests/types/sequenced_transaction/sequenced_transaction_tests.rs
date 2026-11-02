/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::SequencedTransaction;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

fn pids(partitions: &[u32]) -> TestResult<Vec<PartitionId>> {
    partitions
        .iter()
        .map(|&p| Ok(PartitionId::new(p)?))
        .collect()
}

fn refusal(
    reply: u64,
    partitions: Vec<PartitionId>,
    body: Vec<u8>,
) -> TestResult<ClusterErrorEnum> {
    let refused = SequencedTransaction::new(Epoch::new(13)?, Epoch::new(reply)?, partitions, body);
    Ok(refused.err().context("a refusal")?.0)
}

#[test]
fn test_the_limits() {
    assert_eq!(SequencedTransaction::PARTITIONS_MAX, 1_024);
    assert_eq!(SequencedTransaction::BODY_BYTES_MAX, 65_536);
}

#[test]
fn test_a_transaction_keeps_its_fields() -> TestResult {
    let transaction =
        SequencedTransaction::new(Epoch::new(13)?, Epoch::new(12)?, pids(&[3, 1])?, vec![7, 8])?;
    assert_eq!(transaction.epoch().get(), 13);
    assert_eq!(transaction.reply_epoch().get(), 12);
    assert_eq!(transaction.partitions(), pids(&[3, 1])?.as_slice());
    assert_eq!(transaction.body(), [7, 8]);
    Ok(())
}

#[test]
fn test_a_reply_epoch_equal_to_its_epoch_is_taken() -> TestResult {
    let transaction =
        SequencedTransaction::new(Epoch::new(13)?, Epoch::new(13)?, pids(&[1])?, vec![])?;
    assert_eq!(transaction.reply_epoch(), transaction.epoch());
    Ok(())
}

#[test]
fn test_no_partition_is_refused() -> TestResult {
    assert_eq!(refusal(12, vec![], vec![])?, ClusterErrorEnum::NoPartitions);
    Ok(())
}

#[test]
fn test_the_most_partitions_and_body_bytes_are_taken() -> TestResult {
    let partitions: Vec<u32> = (1..=1_024).collect();
    let transaction = SequencedTransaction::new(
        Epoch::new(13)?,
        Epoch::new(12)?,
        pids(&partitions)?,
        vec![0; 65_536],
    )?;
    assert_eq!(transaction.partitions().len(), 1_024);
    assert_eq!(transaction.body().len(), 65_536);
    Ok(())
}

#[test]
fn test_too_many_partitions_or_body_bytes_are_refused() -> TestResult {
    let partitions: Vec<u32> = (1..=1_025).collect();
    assert_eq!(
        refusal(12, pids(&partitions)?, vec![])?,
        ClusterErrorEnum::TooMany {
            what: "partitions of a transaction",
            count: 1_025,
            max: 1_024
        }
    );
    assert_eq!(
        refusal(12, pids(&[1])?, vec![0; 65_537])?,
        ClusterErrorEnum::TooMany {
            what: "bytes of a transaction's body",
            count: 65_537,
            max: 65_536
        }
    );
    Ok(())
}

#[test]
fn test_the_first_partition_named_twice_is_named() -> TestResult {
    assert_eq!(
        refusal(12, pids(&[3, 1, 3])?, vec![])?,
        ClusterErrorEnum::DuplicatePartition { partition: 3 }
    );
    // 2 repeats before 1 does.
    assert_eq!(
        refusal(12, pids(&[1, 2, 2, 1])?, vec![])?,
        ClusterErrorEnum::DuplicatePartition { partition: 2 }
    );
    Ok(())
}

#[test]
fn test_a_reply_epoch_past_its_epoch_is_refused() -> TestResult {
    assert_eq!(
        refusal(14, pids(&[1])?, vec![])?,
        ClusterErrorEnum::EpochAhead {
            epoch: 14,
            latest: 13
        }
    );
    Ok(())
}

#[test]
fn test_the_checks_run_in_order() -> TestResult {
    // Empty before the epoch; too many before a duplicate; a duplicate before the epoch.
    assert_eq!(refusal(14, vec![], vec![])?, ClusterErrorEnum::NoPartitions);
    let mut many: Vec<u32> = (1..=1_025).collect();
    many.push(1);
    let ClusterErrorEnum::TooMany { .. } = refusal(12, pids(&many)?, vec![])? else {
        return Err(TestError::new("too many before a duplicate"));
    };
    assert_eq!(
        refusal(14, pids(&[1, 1])?, vec![])?,
        ClusterErrorEnum::DuplicatePartition { partition: 1 }
    );
    Ok(())
}
