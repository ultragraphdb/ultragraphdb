/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::{
    CatalogVersion, Close, SequencedTransaction, SequencerRecord,
};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// The close of epoch 1 at 1,000 µs announcing version 7 of namespace 3 and evicting session 0x0102…10, by hand.
/// Offsets: the kind 0, the epoch 1, the time 9, the catalog count 17, the version 19 and 27, the eviction count 35,
/// the session 37.
const CLOSE: [u8; 53] = [
    0x01, // a close
    0, 0, 0, 0, 0, 0, 0, 1, // epoch 1
    0, 0, 0, 0, 0, 0, 0x03, 0xE8, // 1,000 µs
    0, 1, // one version
    0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 7, // namespace 3, version 7
    0, 1, // one eviction
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
];

/// The transaction placed in epoch 13 with reply epoch 12, naming partitions 3 and 1, its body `AA BB`, by hand.
/// Offsets: the kind 0, the epoch 1, the reply epoch 9, the partition count 17, the partitions 19 and 23, the body's
/// length 27, the body 31.
const TRANSACTION: [u8; 33] = [
    0x02, // a transaction
    0, 0, 0, 0, 0, 0, 0, 0x0D, // epoch 13
    0, 0, 0, 0, 0, 0, 0, 0x0C, // reply epoch 12
    0, 2, // two partitions
    0, 0, 0, 3, 0, 0, 0, 1, // partitions 3 and 1
    0, 0, 0, 2, // two body bytes
    0xAA, 0xBB,
];

fn close() -> TestResult<SequencerRecord> {
    Ok(SequencerRecord::Close(Close::new(
        Epoch::new(1)?,
        1_000,
        vec![CatalogVersion::new(Namespace::new(3), 7)],
        vec![0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10],
    )?))
}

fn transaction() -> TestResult<SequencerRecord> {
    Ok(SequencerRecord::Transaction(SequencedTransaction::new(
        Epoch::new(13)?,
        Epoch::new(12)?,
        vec![PartitionId::new(3)?, PartitionId::new(1)?],
        vec![0xAA, 0xBB],
    )?))
}

fn encoded(record: &SequencerRecord) -> Vec<u8> {
    let mut out = Vec::new();
    record.encode(&mut out);
    out
}

fn patched(bytes: &[u8], offset: usize, with: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for (i, byte) in with.iter().enumerate() {
        if let Some(slot) = out.get_mut(offset + i) {
            *slot = *byte;
        }
    }
    out
}

fn refusal(bytes: &[u8]) -> TestResult<ClusterErrorEnum> {
    Ok(SequencerRecord::decode(bytes).err().context("a refusal")?.0)
}

fn malformed(what: &'static str, offset: usize) -> ClusterErrorEnum {
    ClusterErrorEnum::Malformed { what, offset }
}

#[test]
fn test_records_encode_as_written_by_hand() -> TestResult {
    assert_eq!(encoded(&close()?), CLOSE);
    assert_eq!(encoded(&transaction()?), TRANSACTION);
    Ok(())
}

#[test]
fn test_the_hand_written_records_decode() -> TestResult {
    assert_eq!(SequencerRecord::decode(&CLOSE)?, close()?);
    assert_eq!(SequencerRecord::decode(&TRANSACTION)?, transaction()?);
    Ok(())
}

#[test]
fn test_records_at_their_limits_round_trip() -> TestResult {
    let catalog: Vec<_> = (0..256)
        .map(|v| CatalogVersion::new(Namespace::new(v), v))
        .collect();
    let largest = SequencerRecord::Close(Close::new(
        Epoch::MAX,
        u64::MAX,
        catalog,
        (0..4_096).collect(),
    )?);
    let partitions = (1..=1_024)
        .map(PartitionId::new)
        .collect::<Result<Vec<_>, _>>()?;
    let widest = SequencerRecord::Transaction(SequencedTransaction::new(
        Epoch::MAX,
        Epoch::new(0)?,
        partitions,
        vec![0x5A; 65_536],
    )?);
    let empty = SequencerRecord::Close(Close::new(Epoch::new(0)?, 0, vec![], vec![])?);
    for record in [largest, widest, empty] {
        let bytes = encoded(&record);
        assert_eq!(SequencerRecord::decode(&bytes)?, record);
    }
    Ok(())
}

#[test]
fn test_encoding_appends_to_the_buffer() -> TestResult {
    let mut out = vec![0xEE];
    transaction()?.encode(&mut out);
    assert_eq!(out.first(), Some(&0xEE));
    assert_eq!(out.get(1..), Some(&TRANSACTION[..]));
    Ok(())
}

#[test]
fn test_an_unknown_kind_is_refused() -> TestResult {
    assert_eq!(refusal(&[])?, malformed("a kind byte, 1 or 2", 0));
    for kind in [0, 3, 0xFF] {
        assert_eq!(
            refusal(&patched(&CLOSE, 0, &[kind]))?,
            malformed("a kind byte, 1 or 2", 0)
        );
    }
    Ok(())
}

#[test]
fn test_an_epoch_past_42_bits_is_refused() -> TestResult {
    let past = [0, 0, 0x04, 0, 0, 0, 0, 0];
    assert_eq!(
        refusal(&patched(&CLOSE, 1, &past))?,
        malformed("an epoch of 42 bits", 1)
    );
    assert_eq!(
        refusal(&patched(&TRANSACTION, 1, &past))?,
        malformed("an epoch of 42 bits", 1)
    );
    assert_eq!(
        refusal(&patched(&TRANSACTION, 9, &past))?,
        malformed("an epoch of 42 bits", 9)
    );
    Ok(())
}

#[test]
fn test_partition_zero_is_refused() -> TestResult {
    assert_eq!(
        refusal(&patched(&TRANSACTION, 26, &[0]))?,
        malformed("a partition from 1", 23)
    );
    Ok(())
}

#[test]
fn test_counts_past_their_limits_are_refused() -> TestResult {
    assert_eq!(
        refusal(&patched(&CLOSE, 17, &[1, 1]))?,
        ClusterErrorEnum::TooMany {
            what: "catalog versions in a close",
            count: 257,
            max: 256
        }
    );
    assert_eq!(
        refusal(&patched(&CLOSE, 35, &[0x10, 1]))?,
        ClusterErrorEnum::TooMany {
            what: "session evictions in a close",
            count: 4_097,
            max: 4_096
        }
    );
    assert_eq!(
        refusal(&patched(&TRANSACTION, 17, &[4, 1]))?,
        ClusterErrorEnum::TooMany {
            what: "partitions of a transaction",
            count: 1_025,
            max: 1_024
        }
    );
    assert_eq!(
        refusal(&patched(&TRANSACTION, 27, &[0, 1, 0, 1]))?,
        ClusterErrorEnum::TooMany {
            what: "bytes of a transaction's body",
            count: 65_537,
            max: 65_536
        }
    );
    Ok(())
}

#[test]
fn test_a_decoded_transaction_is_checked_as_a_built_one() -> TestResult {
    // No partition: the count 0, the two partitions gone.
    let mut empty = TRANSACTION.get(..17).context("the head")?.to_vec();
    empty.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    assert_eq!(refusal(&empty)?, ClusterErrorEnum::NoPartitions);
    assert_eq!(
        refusal(&patched(&TRANSACTION, 22, &[1]))?,
        ClusterErrorEnum::DuplicatePartition { partition: 1 }
    );
    assert_eq!(
        refusal(&patched(&TRANSACTION, 16, &[0x0E]))?,
        ClusterErrorEnum::EpochAhead {
            epoch: 14,
            latest: 13
        }
    );
    Ok(())
}

#[test]
fn test_bytes_after_the_end_are_refused() -> TestResult {
    for record in [&CLOSE[..], &TRANSACTION[..]] {
        let mut bytes = record.to_vec();
        bytes.push(0);
        assert_eq!(
            refusal(&bytes)?,
            malformed("the end of the record", record.len())
        );
    }
    Ok(())
}

#[test]
fn test_every_truncation_is_refused() -> TestResult {
    for record in [&CLOSE[..], &TRANSACTION[..]] {
        for len in 0..record.len() {
            let refused = refusal(record.get(..len).context("a prefix")?)?;
            let ClusterErrorEnum::Malformed { offset, .. } = refused else {
                return Err(TestError::new(format!("length {len}: {refused:?}")));
            };
            assert!(offset <= len, "length {len}: offset {offset}");
        }
    }
    Ok(())
}

#[test]
fn test_a_truncated_value_names_its_field() -> TestResult {
    assert_eq!(
        refusal(CLOSE.get(..12).context("a prefix")?)?,
        malformed("a close's time", 9)
    );
    assert_eq!(
        refusal(TRANSACTION.get(..32).context("a prefix")?)?,
        malformed("a transaction's body", 31)
    );
    Ok(())
}

#[test]
fn test_every_fuzz_seed_decodes_as_its_name_says() -> TestResult {
    // The seeds of `record_decode`, encoded by testdata/seeds.py from the format's documentation alone.
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("record_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 10);
    for (name, bytes) in seeds {
        match SequencerRecord::decode(&bytes) {
            Ok(record) => {
                assert!(!name.starts_with("malformed"), "{name} accepted");
                assert_eq!(encoded(&record), bytes, "{name}");
            }
            Err(error) => {
                assert!(name.starts_with("malformed"), "{name}: {error}");
                assert!(
                    matches!(error.0, ClusterErrorEnum::Malformed { .. }),
                    "{name}: {error}"
                );
            }
        }
    }
    Ok(())
}
