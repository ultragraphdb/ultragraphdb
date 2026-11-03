/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::{
    CatalogVersion, Close, SequencedTransaction, SequencerRecord, SequencerSnapshot,
};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;

/// The snapshot's encoding.
fn encoded(snapshot: &SequencerSnapshot) -> Vec<u8> {
    let mut out = Vec::new();
    snapshot.encode(&mut out);
    out
}

/// The refusal of decoding `bytes`.
fn refusal(bytes: &[u8]) -> TestResult<ClusterErrorEnum> {
    Ok(SequencerSnapshot::decode(bytes)
        .err()
        .context("a refusal")?
        .0)
}

/// The snapshot at position 6 of the closes of 3 (at 100 µs, announcing version 2 of namespace 1) and 4 (at 200 µs,
/// carrying the eviction of session 7) and the transaction at position 5, placed in epoch 5.
fn sample() -> TestResult<SequencerSnapshot> {
    let closes = vec![
        Close::new(
            Epoch::new(3)?,
            100,
            vec![CatalogVersion::new(Namespace::new(1), 2)],
            Vec::new(),
        )?,
        Close::new(Epoch::new(4)?, 200, Vec::new(), vec![7])?,
    ];
    let transaction = SequencedTransaction::new(
        Epoch::new(5)?,
        Epoch::new(4)?,
        vec![PartitionId::new(2)?],
        vec![0xAB],
    )?;
    Ok(SequencerSnapshot::new(
        Position::new(6),
        closes,
        vec![(Position::new(5), transaction)],
    )?)
}

/// The record encodings of the sample, written by hand from `SequencerRecord::encode`'s documentation.
fn sample_bytes() -> Vec<u8> {
    // The close of 3: kind 1, epoch 3, time 100, one version (namespace 1, version 2), no eviction.
    let mut close3 = vec![1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 100, 0, 1];
    close3.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0]);
    // The close of 4: kind 1, epoch 4, time 200, no version, one eviction, session 7.
    let mut close4 = vec![
        1, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 200, 0, 0, 0, 1,
    ];
    close4.extend_from_slice(&7_u128.to_be_bytes());
    // The transaction: kind 2, epoch 5, reply epoch 4, one partition, 2, body length 1, 0xAB.
    let transaction = vec![
        2, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 4, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1, 0xAB,
    ];
    let mut out = vec![1, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 2];
    for record in [&close3, &close4] {
        out.extend_from_slice(
            &u32::try_from(record.len())
                .unwrap_or_default()
                .to_be_bytes(),
        );
        out.extend_from_slice(record);
    }
    out.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 5]);
    out.extend_from_slice(
        &u32::try_from(transaction.len())
            .unwrap_or_default()
            .to_be_bytes(),
    );
    out.extend_from_slice(&transaction);
    out
}

#[test]
fn test_a_snapshot_encodes_as_written_by_hand() -> TestResult {
    let snapshot = sample()?;
    assert_eq!(encoded(&snapshot), sample_bytes());
    assert_eq!(SequencerSnapshot::decode(&sample_bytes())?, snapshot);
    Ok(())
}

#[test]
fn test_an_empty_snapshot_round_trips() -> TestResult {
    let empty = SequencerSnapshot::new(Position::ZERO, Vec::new(), Vec::new())?;
    let bytes = encoded(&empty);
    assert_eq!(
        bytes,
        vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(SequencerSnapshot::decode(&bytes)?, empty);
    Ok(())
}

#[test]
fn test_encoding_appends_to_the_buffer() -> TestResult {
    let mut out = vec![0xAA];
    sample()?.encode(&mut out);
    assert_eq!(out.first(), Some(&0xAA));
    assert_eq!(out.get(1..), Some(sample_bytes().as_slice()));
    Ok(())
}

#[test]
fn test_every_truncation_is_refused() -> TestResult {
    let bytes = sample_bytes();
    for cut in 0..bytes.len() {
        let refused = refusal(bytes.get(..cut).context("a prefix")?)?;
        assert!(
            matches!(refused, ClusterErrorEnum::Malformed { offset, .. } if offset <= cut),
            "cut at {cut}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn test_malformed_snapshots_are_refused_naming_the_value() -> TestResult {
    let mut format = sample_bytes();
    if let Some(byte) = format.first_mut() {
        *byte = 2;
    }
    assert_eq!(
        refusal(&format)?,
        ClusterError::malformed("the format byte 1", 0).0
    );
    let mut trailing = sample_bytes();
    let end = trailing.len();
    trailing.push(0);
    assert_eq!(
        refusal(&trailing)?,
        ClusterError::malformed("the end of the snapshot", end).0
    );
    // A transaction's record in a close's place: the first record's kind byte (offset 17) set to 2.
    let mut kind = sample_bytes();
    let mut transaction = Vec::new();
    SequencerRecord::Transaction(SequencedTransaction::new(
        Epoch::new(1)?,
        Epoch::new(1)?,
        vec![PartitionId::new(1)?],
        Vec::new(),
    )?)
    .encode(&mut transaction);
    kind.truncate(13);
    kind.extend_from_slice(&u32::try_from(transaction.len())?.to_be_bytes());
    kind.extend_from_slice(&transaction);
    assert_eq!(refusal(&kind)?, ClusterError::malformed("a close", 17).0);
    // Closes out of order: the second close's epoch (offset 21 + 4 + 8) made 5.
    let mut order = sample_bytes();
    let second_epoch = 13 + 4 + 37 + 4 + 8;
    if let Some(byte) = order.get_mut(second_epoch) {
        *byte = 5;
    }
    assert_eq!(
        refusal(&order)?,
        ClusterError::malformed("a close one after the one before it", 54).0
    );
    // A record whose own bytes are refused, as the record decoder names them, from the snapshot's offset.
    let mut inner = sample_bytes();
    if let Some(byte) = inner.get_mut(17) {
        *byte = 9;
    }
    assert_eq!(
        refusal(&inner)?,
        ClusterError::malformed("a kind byte, 1 or 2", 17).0
    );
    Ok(())
}

#[test]
fn test_every_fuzz_seed_decodes_as_its_name_says() -> TestResult {
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("snapshot_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 3);
    for (name, bytes) in seeds {
        match SequencerSnapshot::decode(&bytes) {
            Ok(snapshot) => {
                assert!(!name.starts_with("malformed"), "{name} accepted");
                assert_eq!(encoded(&snapshot), bytes, "{name}");
            }
            Err(error) => assert!(name.starts_with("malformed"), "{name}: {error}"),
        }
    }
    Ok(())
}

#[test]
fn test_the_fuzz_seeds_are_the_encoders() -> TestResult {
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("snapshot_decode");
    let mut seeds: Vec<Vec<u8>> = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?
        .into_iter()
        .filter(|(name, _)| !name.starts_with("malformed"))
        .map(|(_, bytes)| bytes)
        .collect();
    let mut expected = vec![
        encoded(&SequencerSnapshot::new(
            Position::ZERO,
            Vec::new(),
            Vec::new(),
        )?),
        sample_bytes(),
    ];
    seeds.sort();
    expected.sort();
    assert_eq!(seeds, expected);
    Ok(())
}

#[test]
fn test_a_records_refusal_is_named_at_its_offset_in_the_snapshot() -> TestResult {
    // The first close's epoch (the record's byte 1, the snapshot's 18) past 42 bits.
    let mut epoch = sample_bytes();
    if let Some(byte) = epoch.get_mut(18) {
        *byte = 0xFF;
    }
    assert_eq!(
        refusal(&epoch)?,
        ClusterError::malformed("an epoch of 42 bits", 18).0
    );
    Ok(())
}
