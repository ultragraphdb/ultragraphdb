/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Opening the write-ahead log: a fresh log, the superblock's outcomes, and the recovery's scan of the segments with
//! its corruption rules; then the log under the simulated storage's random faults and power losses.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, SubscriberExt};
use ultragraphdb_error_log::{LogError, LogErrorEnum};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_transactions_log::{
    Damage, DurableStorage, FaultMode, Recovery, SimStorage, StorageOperation, Superblock,
    SuperblockRead, Truncation, Wal,
};

use crate::utils::helpers::{
    CLUSTER, FULL_BLOCK, FULL_TWO_BLOCKS, ONE_BLOCK, TWO_BLOCKS, append, append_rounds,
    append_synced, block_on, body, config, derivation_of, dir, open, segment, spare, superblock,
};

/// Asserts `error` is the storage's failure of `operation`, as the simulated storage reports an injected fault.
fn assert_io(error: &LogError, operation: &str) {
    assert!(
        matches!(
            &error.0,
            LogErrorEnum::Io { operation: named, kind: std::io::ErrorKind::Other, .. } if *named == operation
        ),
        "not an I/O error of {operation}: {error:?}"
    );
}

fn writes_in(storage: &SimStorage, from: usize) -> usize {
    storage.trace()[from..]
        .iter()
        .filter(|(op, _)| {
            matches!(
                op,
                StorageOperation::Write
                    | StorageOperation::Create
                    | StorageOperation::Rename
                    | StorageOperation::Remove
                    | StorageOperation::Preallocate
            )
        })
        .count()
}

#[test]
fn test_a_fresh_log_writes_its_superblock_durably() -> TestResult {
    let storage = SimStorage::new(1);
    let (wal, recovery) = open(&storage).context("opened")?;
    assert_eq!(recovery.checkpoint(), Position::ZERO);
    assert_eq!(recovery.committed(), Position::ZERO);
    assert_eq!(recovery.truncation(), None);
    assert_eq!(wal.committed(), Position::ZERO);
    assert_eq!(wal.tail(), (Position::ZERO, 0));
    assert_eq!(wal.incarnation(), 1);
    assert_eq!(wal.checkpoint(), Position::ZERO);
    storage.power_loss();
    assert_eq!(
        block_on(Superblock::read(&storage, &dir())).context("read")?,
        SuperblockRead::Present(Superblock::new(1, CLUSTER, 1, Position::ZERO, 0, 0))
    );
    // Opened again, the log is the same log: no second superblock is written.
    let before = storage.trace().len();
    let (reopened, _) = open(&storage).context("reopened")?;
    assert_eq!(reopened.incarnation(), 1);
    assert!(
        !storage.trace()[before..]
            .iter()
            .any(|(op, _)| *op == StorageOperation::Rename)
    );
    Ok(())
}

#[test]
fn test_reopening_recovers_the_synced_rounds_and_drops_the_staged() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    let checksums = append_synced(&mut wal, 1, 5, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 6, ONE_BLOCK).context("staged")?,
        Position::new(6)
    );
    drop(wal);
    let (reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::new(5));
    assert_eq!(recovery.truncation(), None);
    assert_eq!(reopened.tail(), (Position::new(5), checksums[4]));
    assert_eq!(reopened.committed(), Position::new(5));
    Ok(())
}

#[test]
fn test_a_corrupt_superblock_copy_refuses_the_start_and_writes_nothing() -> TestResult {
    for copy in ['a', 'b'] {
        let storage = SimStorage::new(1);
        let (mut wal, _) = open(&storage).context("opened")?;
        append_rounds(&mut wal, 1, 6, ONE_BLOCK)?;
        block_on(wal.trim_through(Position::new(2))).context("copy b written")?;
        drop(wal);
        storage
            .corrupt(&superblock(copy), 20, 0x04)
            .context("flipped")?;
        let before = storage.trace().len();
        assert_eq!(
            open(&storage).err(),
            Some(LogError::superblock_unreadable(copy))
        );
        assert_eq!(writes_in(&storage, before), 0, "copy {copy}");
    }
    Ok(())
}

#[test]
fn test_a_superblock_of_another_cluster_diverges() -> TestResult {
    let storage = SimStorage::new(1);
    storage
        .put_file(
            &superblock('a'),
            &Superblock::new(1, CLUSTER ^ 1, 1, Position::ZERO, 0, 0).encode(),
        )
        .context("put")?;
    assert_eq!(
        open(&storage).err(),
        Some(LogError::diverged("the cluster", CLUSTER, CLUSTER ^ 1))
    );
    Ok(())
}

#[test]
fn test_segments_without_a_superblock_are_refused_not_overwritten() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 6, ONE_BLOCK)?;
    drop(wal);
    for copy in ['a', 'b'] {
        // A copy never written has nothing to remove.
        let _removed: std::io::Result<()> = block_on(storage.remove_file(&superblock(copy)));
    }
    let before = storage.trace().len();
    assert_eq!(
        open(&storage).err(),
        Some(LogError::diverged("segments without a superblock", 0, 2))
    );
    assert_eq!(writes_in(&storage, before), 0);
    Ok(())
}

#[test]
fn test_open_finishes_an_interrupted_trim_and_sweeps_temporaries() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    let checksums = append_synced(&mut wal, 1, 10, ONE_BLOCK)?;
    drop(wal);
    // The checkpoint at 8 was recorded, and the power was lost before segments 0 and 1 were removed.
    storage
        .put_file(
            &superblock('b'),
            &Superblock::new(2, CLUSTER, 1, Position::new(8), checksums[7], 2).encode(),
        )
        .context("put")?;
    storage
        .put_file(&dir().join("superblock.a.tmp"), b"half")
        .context("put")?;
    let (mut reopened, recovery) = open(&storage).context("opened")?;
    assert_eq!(recovery.checkpoint(), Position::new(8));
    assert_eq!(recovery.committed(), Position::new(10));
    assert_eq!(storage.stable_contents(&segment(0)), None);
    assert_eq!(storage.stable_contents(&segment(1)), None);
    assert_eq!(
        storage.stable_contents(&dir().join("superblock.a.tmp")),
        None
    );
    let rounds = block_on(reopened.read(Position::new(9), u32::MAX)).context("read")?;
    assert_eq!(rounds.len(), 2);
    Ok(())
}

#[test]
fn test_other_names_in_the_directory_are_not_segments() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
    drop(wal);
    for name in [
        "x.segment",
        "123.segment",
        "0000000000000000000a.segment",
        "notes",
    ] {
        storage
            .put_file(&dir().join(name), b"junk")
            .context("put")?;
    }
    let (_, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::new(2));
    assert_eq!(
        storage.contents(&dir().join("x.segment")),
        Some(b"junk".to_vec())
    );
    Ok(())
}

/// A failed listing of the log's directory, the open's third operation after creating the directory and syncing its
/// parent, is an I/O error of the listing, and nothing is written: the next open creates a fresh log.
#[test]
fn test_a_failed_listing_is_an_io_error() -> TestResult {
    let storage = SimStorage::new(1);
    storage.fail_at(2, FaultMode::Before);
    let refusal = open(&storage).err().context("refused")?;
    assert_io(&refusal, "list");
    assert_eq!(storage.trace()[2].0, StorageOperation::List);
    assert_eq!(writes_in(&storage, 0), 0, "nothing written");
    let (_wal, recovery) = open(&storage).context("the next open succeeds")?;
    assert_eq!(recovery.committed(), Position::ZERO);
    Ok(())
}

#[test]
fn test_a_failed_directory_creation_is_an_io_error() -> TestResult {
    let storage = SimStorage::new(1);
    storage.fail_at(0, FaultMode::Before);
    let refusal = open(&storage).err().context("refused")?;
    assert!(
        matches!(
            refusal.0,
            LogErrorEnum::Io {
                operation: "create directory",
                ..
            }
        ),
        "{refusal:?}"
    );
    // The fault fired once and wrote nothing: the next open creates a fresh log.
    let (wal, recovery) = open(&storage).context("the next open succeeds")?;
    assert_eq!(
        (
            recovery.checkpoint(),
            recovery.committed(),
            recovery.truncation()
        ),
        (Position::ZERO, Position::ZERO, None)
    );
    assert_eq!(wal.tail(), (Position::ZERO, 0));
    Ok(())
}

#[test]
fn test_a_tail_torn_in_its_header_is_named_by_its_position() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    drop(wal);
    // Round 3's header no longer verifies and nothing follows it: a torn tail, named as the round expected there.
    storage
        .corrupt(&segment(0), 8_192 + 50, 0x01)
        .context("flipped")?;
    let (_, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery.truncation(),
        Some(Truncation::new(Position::new(3), 0, 8_192, Damage::Header))
    );
    assert_eq!(recovery.committed(), Position::new(2));
    Ok(())
}

/// A Round frame claiming position 0 at a fresh log's head is no round of the log: header damage with nothing after it,
/// a torn tail named as the round expected there, round 1, and the log stays empty.
#[test]
fn test_a_round_claiming_position_zero_at_a_fresh_logs_head_is_header_damage() -> TestResult {
    use ultragraphdb_traits_log::RoundRef;
    use ultragraphdb_transactions_log::encode_round;
    let storage = SimStorage::new(1);
    let (wal, _) = open(&storage).context("opened")?;
    drop(wal);
    let data = body(0, ONE_BLOCK);
    let (frame, _) = encode_round(
        CLUSTER,
        0,
        1,
        &RoundRef::new(Position::ZERO, 0, 2, 1, derivation_of(0), &data),
    )
    .context("encoded")?;
    // A fresh log has created no segment yet: segment 0 is laid down holding the frame, then zeros, four blocks long.
    let mut bytes = frame.as_slice().to_vec();
    bytes.resize(16_384, 0);
    storage.put_file(&segment(0), &bytes).context("put")?;
    let (reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery.truncation(),
        Some(Truncation::new(Position::new(1), 0, 0, Damage::Header))
    );
    assert_eq!(recovery.committed(), Position::ZERO);
    assert_eq!(reopened.tail(), (Position::ZERO, 0));
    Ok(())
}

/// The stable contents of segment 0 with `block` after its first `len` bytes, put in its place.
fn put_segment_0(storage: &SimStorage, len: usize, block: &[u8]) -> TestResult {
    let mut bytes = storage.stable_contents(&segment(0)).context("segment 0")?;
    bytes.truncate(len);
    bytes.extend_from_slice(block);
    storage.put_file(&segment(0), &bytes).context("put")?;
    Ok(())
}

#[test]
fn test_preallocated_zeros_are_not_read_as_rounds() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    let checksums = append_synced(&mut wal, 1, 3, ONE_BLOCK)?;
    drop(wal);
    // Zeros after the rounds, as a segment holds them where a recovery cleared a torn tail.
    put_segment_0(&storage, 12_288, &[0u8; 4_096])?;
    let (reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::new(3));
    assert_eq!(recovery.truncation(), None);
    assert_eq!(reopened.tail(), (Position::new(3), checksums[2]));
    Ok(())
}

#[test]
fn test_a_fresh_segment_ends_at_its_last_round() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    drop(wal);
    // Three one-block rounds: the segment is three blocks long, nothing after them.
    assert_eq!(
        storage.stable_contents(&segment(0)).map(|b| b.len()),
        Some(12_288)
    );
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::new(3));
    assert_eq!(recovery.truncation(), None);
    let checksums = append_synced(&mut reopened, 4, 4, ONE_BLOCK)?;
    let bytes = storage.stable_contents(&segment(0)).context("segment 0")?;
    assert_eq!(bytes.len(), 16_384);
    assert_eq!(
        &bytes[12_288 + 256..12_288 + 1_256],
        body(4, ONE_BLOCK).as_slice()
    );
    drop(reopened);
    let (restarted, recovered) = open(&storage).context("reopened")?;
    assert_eq!(recovered.committed(), Position::new(4));
    assert_eq!(restarted.tail(), (Position::new(4), checksums[0]));
    Ok(())
}

#[test]
fn test_a_torn_tail_in_a_short_segment_is_cleared_within_its_length() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 1, ONE_BLOCK)?;
    append_rounds(&mut wal, 2, 2, FULL_TWO_BLOCKS)?;
    drop(wal);
    // Round 2's first chunk is damaged and nothing follows: a torn tail at 4,096, its second block left behind it.
    storage
        .corrupt(&segment(0), 4_096 + 300, 0x01)
        .context("flipped")?;
    let (_, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery.truncation().map(|t| (t.position(), t.offset())),
        Some((Position::new(2), 4_096))
    );
    let bytes = storage.stable_contents(&segment(0)).context("segment 0")?;
    assert_eq!(bytes.len(), 12_288, "cleared up to its length, not grown");
    assert!(bytes[8_192..].iter().all(|b| *b == 0), "cleared");
    assert!(
        bytes[4_096..8_192].iter().any(|b| *b != 0),
        "the damaged block is kept"
    );
    Ok(())
}

#[test]
fn test_a_torn_tail_is_truncated_reported_until_overwritten() -> TestResult {
    let storage = SimStorage::new(3);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 4, FULL_BLOCK).context("staged")?,
        Position::new(4)
    );
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    let torn = block_on(wal.sync())
        .err()
        .context("the torn write is reported")?;
    assert_io(&torn, "write");
    drop(wal);
    // The process restarts with the torn bytes in place.
    let (reopened, recovery) = open(&storage).context("reopened")?;
    let truncation = recovery.truncation().context("a truncation")?;
    assert_eq!(truncation.position(), Position::new(4));
    assert_eq!((truncation.segment(), truncation.offset()), (0, 12_288));
    assert!(
        matches!(truncation.damage(), Damage::Chunk { position, index: 0, start: 0, end: 3_840 } if position == Position::new(4)),
        "{:?}",
        truncation.damage()
    );
    assert_eq!(recovery.committed(), Position::new(3));
    // The torn round's block is left for the next append to overwrite; until then each open reports it again.
    drop(reopened);
    let (mut restarted, again) = open(&storage).context("reopened")?;
    assert_eq!(again.truncation(), recovery.truncation());
    append_rounds(&mut restarted, 4, 4, ONE_BLOCK)?;
    let bytes = storage.stable_contents(&segment(0)).context("segment 0")?;
    assert_eq!(
        &bytes[12_288 + 256..12_288 + 1_256],
        body(4, ONE_BLOCK).as_slice()
    );
    assert!(
        bytes[12_288 + 1_256..].iter().all(|b| *b == 0),
        "nothing of the torn round is left"
    );
    append_rounds(&mut restarted, 5, 5, ONE_BLOCK)?;
    drop(restarted);
    let (_, recovered) = open(&storage).context("reopened")?;
    assert_eq!(recovered.truncation(), None);
    assert_eq!(recovered.committed(), Position::new(5));
    Ok(())
}

#[test]
fn test_a_torn_round_starting_a_segment_is_truncated_there() -> TestResult {
    let storage = SimStorage::new(3);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 4, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 5, FULL_BLOCK).context("staged")?,
        Position::new(5)
    );
    // Segment 1's creation, preallocation, sync and directory sync, then the round's write.
    storage.fail_at(storage.trace().len() + 4, FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(5)));
    drop(wal);
    storage.power_loss();
    let (_, recovery) = open(&storage).context("reopened")?;
    let truncation = recovery.truncation().context("a truncation")?;
    assert_eq!(truncation.position(), Position::new(5));
    assert_eq!((truncation.segment(), truncation.offset()), (1, 0));
    assert_eq!(recovery.committed(), Position::new(4));
    Ok(())
}

#[test]
fn test_a_silent_torn_write_is_caught_and_named() -> TestResult {
    let storage = SimStorage::new(5);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 3, FULL_TWO_BLOCKS).context("staged")?,
        Position::new(3)
    );
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(3)), "acknowledged");
    drop(wal);
    storage.power_loss();
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    // Round 3's frame fills blocks 2 and 3 of segment 0, its 16-byte chunk table in the write's last sector: a tear
    // keeps at least one sector and never all, so the table is always lost and its checksum fails before any chunk.
    assert_eq!(
        recovery.truncation(),
        Some(Truncation::new(
            Position::new(3),
            0,
            8_192,
            Damage::ChunkTable {
                position: Position::new(3)
            }
        ))
    );
    assert_eq!(recovery.committed(), Position::new(2));
    assert_eq!(
        block_on(reopened.read(Position::new(3), u32::MAX)),
        Ok(Vec::new())
    );
    assert_eq!(append(&mut reopened, 3, ONE_BLOCK), Ok(Position::new(3)));
    Ok(())
}

#[test]
fn test_a_truncation_is_a_warning_event() -> TestResult {
    let storage = SimStorage::new(3);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 3, FULL_BLOCK).context("staged")?,
        Position::new(3)
    );
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    assert_io(&block_on(wal.sync()).err().context("torn")?, "write");
    drop(wal);
    let events = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture(events.clone()));
    // Tracing caches a callsite's interest process-wide: a second dispatcher kept alive, and the cache rebuilt under
    // this one, keep a callsite another test thread reached first from being cached as uninteresting (as
    // ultragraphdb_store_sim's LogCapture does).
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let _reopened: (Wal<SimStorage>, Recovery) =
        tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            open(&storage).context("reopened")
        })?;
    let events = events.lock().unwrap_or_else(PoisonError::into_inner);
    let warnings: Vec<&BTreeMap<String, String>> = events
        .iter()
        .filter(|(level, _)| *level == tracing::Level::WARN)
        .map(|(_, fields)| fields)
        .collect();
    assert_eq!(warnings.len(), 1, "{events:?}");
    assert_eq!(warnings[0].get("position").map(String::as_str), Some("3"));
    assert_eq!(warnings[0].get("segment").map(String::as_str), Some("0"));
    assert_eq!(warnings[0].get("offset").map(String::as_str), Some("8192"));
    assert!(
        warnings[0]
            .get("message")
            .is_some_and(|m| m.contains("truncat"))
    );
    Ok(())
}

#[test]
fn test_interior_damage_refuses_the_start_naming_the_round() -> TestResult {
    let cases = [
        // A body byte of round 2: chunk 0, body bytes 0 to 1,000.
        (
            4_096 + 256 + 500,
            LogError::corrupt_chunk(2, 0, 4_096, 0, 0, 1_000),
        ),
        // A header byte of round 2.
        (4_096 + 50, LogError::corrupt(2, 0, 4_096)),
    ];
    for (offset, expected) in cases {
        let storage = SimStorage::new(1);
        let (mut wal, _) = open(&storage).context("opened")?;
        append_rounds(&mut wal, 1, 5, ONE_BLOCK)?;
        drop(wal);
        storage
            .corrupt(&segment(0), offset, 0x01)
            .context("flipped")?;
        let before = storage.trace().len();
        assert_eq!(open(&storage).err(), Some(expected));
        assert_eq!(writes_in(&storage, before), 0, "nothing is truncated");
    }
    Ok(())
}

#[test]
fn test_a_chunk_damaged_in_a_later_segment_is_named_with_its_segment() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    append_rounds(&mut wal, 4, 6, TWO_BLOCKS)?;
    drop(wal);
    // Round 4 starts segment 1, round 5 at its byte 8,192; chunk 1 of a 5,000-byte body is 3,840 to 5,000.
    storage
        .corrupt(&segment(1), 8_192 + 256 + 4_000, 0x01)
        .context("flipped")?;
    assert_eq!(
        open(&storage).err(),
        Some(LogError::corrupt_chunk(5, 1, 8_192, 1, 3_840, 5_000))
    );
    Ok(())
}

#[test]
fn test_a_missing_segment_with_rounds_after_it_refuses_the_start() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 12, ONE_BLOCK)?;
    drop(wal);
    block_on(storage.remove_file(&segment(1))).context("removed")?;
    assert_eq!(open(&storage).err(), Some(LogError::corrupt(5, 1, 0)));
    Ok(())
}

#[test]
fn test_junk_where_a_round_did_not_fit_is_passed_over() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    let checksums = append_synced(&mut wal, 4, 4, TWO_BLOCKS)?;
    drop(wal);
    // Round 4 did not fit segment 0's last block; junk lies in that unused block.
    let mut junk = [0u8; 4_096];
    junk[7] = 0x40;
    put_segment_0(&storage, 12_288, &junk)?;
    let (reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.truncation(), None);
    assert_eq!(reopened.tail(), (Position::new(4), checksums[0]));
    Ok(())
}

#[test]
fn test_the_checkpoint_must_match_the_rounds() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    let checksums = append_synced(&mut wal, 1, 6, ONE_BLOCK)?;
    drop(wal);
    storage
        .put_file(
            &superblock('b'),
            &Superblock::new(2, CLUSTER, 1, Position::new(5), 0xBAD, 0).encode(),
        )
        .context("put")?;
    assert_eq!(
        open(&storage).err(),
        Some(LogError::diverged(
            "the checkpoint's checksum",
            0xBAD,
            checksums[4]
        ))
    );
    Ok(())
}

#[test]
fn test_a_torn_round_below_the_checkpoint_leaves_the_log_at_the_checkpoint() -> TestResult {
    let storage = SimStorage::new(9);
    let (mut wal, _) = open(&storage).context("opened")?;
    let mut checksums = append_synced(&mut wal, 1, 4, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 5, FULL_BLOCK).context("staged")?,
        Position::new(5)
    );
    storage.fail_at(storage.trace().len() + 4, FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(5)));
    checksums.push(wal.tail().1);
    // The store synced through 5 and the checkpoint recorded it.
    block_on(wal.trim_through(Position::new(5))).context("trimmed")?;
    drop(wal);
    storage.power_loss();
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        reopened.checkpoint(),
        Position::new(5),
        "the restart at the checkpoint keeps its position"
    );
    assert_eq!(
        recovery.truncation().map(|t| t.position()),
        Some(Position::new(5))
    );
    assert_eq!(recovery.checkpoint(), Position::new(5));
    assert_eq!(recovery.committed(), Position::new(5));
    assert_eq!(reopened.tail(), (Position::new(5), checksums[4]));
    append_rounds(&mut reopened, 6, 7, ONE_BLOCK)?;
    drop(reopened);
    let (mut restarted, recovered) = open(&storage).context("reopened")?;
    assert_eq!(restarted.checkpoint(), Position::new(5));
    assert_eq!(recovered.truncation(), None);
    assert_eq!(recovered.committed(), Position::new(7));
    assert_eq!(
        block_on(restarted.read(Position::new(6), u32::MAX))
            .context("read")?
            .len(),
        2
    );
    Ok(())
}

#[test]
fn test_a_torn_round_at_the_checkpoint_after_rounds_below_it_leaves_the_log_at_the_checkpoint()
-> TestResult {
    // Rounds 1 to 3 and the torn round 4 share segment 0; the checkpoint covers 4.
    let storage = SimStorage::new(9);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 4, FULL_BLOCK).context("staged")?,
        Position::new(4)
    );
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(4)));
    let four = wal.tail().1;
    block_on(wal.trim_through(Position::new(4))).context("trimmed")?;
    drop(wal);
    storage.power_loss();
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery.truncation().map(|t| t.position()),
        Some(Position::new(4))
    );
    assert_eq!(recovery.committed(), Position::new(4));
    assert_eq!(reopened.tail(), (Position::new(4), four));
    append_rounds(&mut reopened, 5, 6, ONE_BLOCK)?;
    drop(reopened);
    let (_, recovered) = open(&storage).context("reopened")?;
    assert_eq!(recovered.truncation(), None);
    assert_eq!(recovered.committed(), Position::new(6));
    Ok(())
}

#[test]
fn test_a_tail_segment_cut_short_is_preallocated_again() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
    drop(wal);
    let mut bytes = storage.contents(&segment(0)).context("segment 0")?;
    bytes.truncate(8_192);
    storage.put_file(&segment(0), &bytes).context("put")?;
    let before = storage.trace().len();
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::new(2));
    // Its blocks are reserved again at the open, and synced; its length is left at its rounds' end.
    let steps = &storage.trace()[before..];
    let reserved = steps
        .iter()
        .position(|step| *step == (StorageOperation::Preallocate, segment(0)))
        .context("preallocated at the open")?;
    assert_eq!(
        steps.get(reserved + 1),
        Some(&(StorageOperation::FileSync, segment(0)))
    );
    assert_eq!(
        storage.stable_contents(&segment(0)).map(|b| b.len()),
        Some(8_192)
    );
    append_rounds(&mut reopened, 3, 4, ONE_BLOCK)?;
    assert_eq!(storage.contents(&segment(0)).map(|b| b.len()), Some(16_384));
    Ok(())
}

#[test]
fn test_a_clean_restart_with_the_tail_in_a_recycled_segment_reports_no_truncation() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    // Two-block rounds, two to a segment: 1 and 2 in segment 0, 3 and 4 in 1, 5 and 6 in 2.
    append_rounds(&mut wal, 1, 6, TWO_BLOCKS)?;
    block_on(wal.trim_through(Position::new(4))).context("trimmed")?;
    // Round 7 takes segment 0's spare as segment 3: its second block held the middle of round 1's body.
    let checksums = append_synced(&mut wal, 7, 7, ONE_BLOCK)?;
    assert_eq!(storage.stable_contents(&spare(0)), None);
    assert!(
        storage.stable_contents(&spare(1)).is_some(),
        "segment 1 is a spare"
    );
    drop(wal);
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.truncation(), None);
    assert_eq!(recovery.committed(), Position::new(7));
    assert_eq!(reopened.tail(), (Position::new(7), checksums[0]));
    let rounds = block_on(reopened.read(Position::new(5), u32::MAX)).context("read")?;
    assert_eq!(rounds.len(), 3);
    // Rounds 8 to 10 fill segment 3; round 11 takes the spare the open found, segment 1's.
    append_rounds(&mut reopened, 8, 11, ONE_BLOCK)?;
    assert_eq!(storage.stable_contents(&spare(1)), None);
    drop(reopened);
    let (_, recovered) = open(&storage).context("reopened")?;
    assert_eq!(recovered.truncation(), None);
    assert_eq!(recovered.committed(), Position::new(11));
    Ok(())
}

/// A log of twelve one-block rounds, synced over segments 0 to 2, and their checksums.
fn twelve() -> TestResult<(SimStorage, Wal<SimStorage>, Vec<u64>)> {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    let checksums = append_synced(&mut wal, 1, 12, ONE_BLOCK)?;
    Ok((storage, wal, checksums))
}

/// After a crash and an open: no report, rounds 1 to `committed` committed, every round above the checkpoint read
/// back with its checksum, and three more rounds that survive the next restart without a report. Returns the
/// checkpoint the recovery found.
fn recovers_cleanly(storage: &SimStorage, checksums: &[u64], case: &str) -> TestResult<Position> {
    let (mut wal, recovery) = open(storage).context("recovers")?;
    assert_eq!(recovery.truncation(), None, "{case}");
    let committed = recovery.committed().get();
    assert!(committed >= 12, "{case}: committed {committed}");
    let from = recovery.checkpoint().get() + 1;
    let rounds = block_on(wal.read(Position::new(from), u32::MAX)).context("read")?;
    let from_index = usize::try_from(from)?;
    let read: Vec<u64> = rounds
        .iter()
        .map(|r| r.checksum())
        .take(12 - from_index + 1)
        .collect();
    assert_eq!(read, checksums[from_index - 1..], "{case}");
    if recovery.checkpoint() == Position::new(8) {
        assert_eq!(storage.contents(&segment(0)), None, "{case}");
        assert_eq!(storage.contents(&segment(1)), None, "{case}");
    }
    append_rounds(&mut wal, committed + 1, committed + 3, ONE_BLOCK)?;
    drop(wal);
    let (_, again) = open(storage).context("reopens")?;
    assert_eq!(again.truncation(), None, "{case}");
    assert_eq!(again.committed(), Position::new(committed + 3), "{case}");
    Ok(recovery.checkpoint())
}

#[test]
fn test_a_crash_after_any_step_of_a_recycling_checkpoint_recovers_without_a_report() -> TestResult {
    let steps = {
        let (storage, mut wal, _) = twelve()?;
        let before = storage.trace().len();
        block_on(wal.trim_through(Position::new(8))).context("trimmed")?;
        storage.trace().len() - before
    };
    // The superblock's five steps, then segments 0 and 1 each zeroed, synced and renamed, then the directory synced.
    assert_eq!(steps, 12);
    // The operation each step's error names, in the order above.
    let operations = [
        "create",
        "write",
        "file sync",
        "rename",
        "directory sync",
        "write",
        "file sync",
        "rename",
        "write",
        "file sync",
        "rename",
        "directory sync",
    ];
    for (step, &operation) in operations.iter().enumerate() {
        for power in [false, true] {
            let (storage, mut wal, checksums) = twelve()?;
            storage.fail_at(storage.trace().len() + step, FaultMode::After);
            let failed = block_on(wal.trim_through(Position::new(8)))
                .err()
                .context("the step fails after its effect")?;
            assert_io(&failed, operation);
            drop(wal);
            if power {
                storage.power_loss();
            }
            let case = format!("step {step}, power loss {power}");
            if recovers_cleanly(&storage, &checksums, &case)? == Position::new(8) {
                // Segment 0's spare became segment 3; segment 1's, recycled by the checkpoint or by the open, is left.
                assert_eq!(
                    storage.stable_contents(&spare(1)),
                    Some(vec![0u8; 16_384]),
                    "{case}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn test_a_crash_after_any_step_of_taking_a_spare_loses_no_acknowledged_round() -> TestResult {
    // The spare renamed, the directory synced, the segment opened, the round written and synced.
    let operations = ["rename", "directory sync", "open", "write", "file sync"];
    for (step, &operation) in operations.iter().enumerate() {
        for power in [false, true] {
            let (storage, mut wal, checksums) = twelve()?;
            block_on(wal.trim_through(Position::new(8))).context("trimmed")?;
            assert!(
                storage.stable_contents(&spare(0)).is_some(),
                "a spare to take"
            );
            assert_eq!(
                append(&mut wal, 13, ONE_BLOCK).context("staged")?,
                Position::new(13)
            );
            storage.fail_at(storage.trace().len() + step, FaultMode::After);
            let failed = block_on(wal.sync())
                .err()
                .context("the step fails after its effect")?;
            assert_io(&failed, operation);
            drop(wal);
            if power {
                storage.power_loss();
            }
            let _recovered: Position = recovers_cleanly(
                &storage,
                &checksums,
                &format!("step {step}, power loss {power}"),
            )?;
        }
    }
    Ok(())
}

#[test]
fn test_without_fallocate_a_new_segment_is_filled_with_zeros_and_warned_once() -> TestResult {
    let storage = SimStorage::new(1);
    storage.refuse_preallocation();
    let (mut wal, _) = open(&storage).context("opened")?;
    let before = storage.trace().len();
    let (appended, warnings) = warnings_during(|| append_synced(&mut wal, 1, 5, ONE_BLOCK));
    let checksums = appended?;
    // Segment 0: created, its reservation refused, zeros written in its place and synced, its entry made durable.
    assert_eq!(
        storage.trace()[before..before + 7],
        [
            (StorageOperation::Create, segment(0)),
            (StorageOperation::Preallocate, segment(0)),
            (StorageOperation::Write, segment(0)),
            (StorageOperation::FileSync, segment(0)),
            (StorageOperation::DirectorySync, dir()),
            (StorageOperation::Write, segment(0)),
            (StorageOperation::FileSync, segment(0)),
        ]
    );
    // Two segments created, one warning, naming the error.
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains_key("error"), "{warnings:?}");
    assert_eq!(
        storage.stable_contents(&segment(1)).map(|b| b.len()),
        Some(16_384)
    );
    drop(wal);
    let (reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.truncation(), None);
    assert_eq!(reopened.tail(), (Position::new(5), checksums[4]));
    Ok(())
}

#[test]
fn test_a_torn_checkpoint_round_after_rounds_below_it_recycles_their_segment_at_the_open()
-> TestResult {
    // Rounds 1 and 2 and the torn round 3 in segment 0, which they do not fill; the checkpoint covers 3.
    let storage = SimStorage::new(9);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 3, FULL_BLOCK).context("staged")?,
        Position::new(3)
    );
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(3)));
    let three = wal.tail().1;
    block_on(wal.trim_through(Position::new(3))).context("trimmed")?;
    assert!(
        storage.contents(&segment(0)).is_some(),
        "the next round's segment is kept"
    );
    drop(wal);
    storage.power_loss();
    // The log continues from the checkpoint in segment 1; segment 0, below it, is recycled by this open.
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery.truncation().map(|t| t.position()),
        Some(Position::new(3))
    );
    assert_eq!(reopened.tail(), (Position::new(3), three));
    assert_eq!(
        reopened.checkpoint(),
        Position::new(3),
        "the restart at the checkpoint keeps its position"
    );
    assert_eq!(storage.stable_contents(&segment(0)), None);
    assert_eq!(storage.stable_contents(&spare(0)), Some(vec![0u8; 16_384]));
    append_rounds(&mut reopened, 4, 4, ONE_BLOCK)?;
    drop(reopened);
    let (_, recovered) = open(&storage).context("reopened")?;
    assert_eq!(recovered.truncation(), None);
    assert_eq!(recovered.committed(), Position::new(4));
    Ok(())
}

/// The first seed from 0 whose storage, after `setup`, holds segment 0 with a length that is not whole blocks.
fn seed_leaving_a_partial_block(
    setup: impl Fn(&SimStorage) -> TestResult,
) -> TestResult<SimStorage> {
    for seed in 0..64 {
        let storage = SimStorage::new(seed);
        setup(&storage)?;
        if storage
            .contents(&segment(0))
            .is_some_and(|bytes| !bytes.len().is_multiple_of(4_096))
        {
            return Ok(storage);
        }
    }
    Err(TestError::new("a seed among 64 tears inside a block"))
}

#[test]
fn test_a_torn_tail_before_a_short_torn_write_is_cleared_to_a_whole_block() -> TestResult {
    // Round 3 torn silently, then round 4's extending write torn short, inside a block.
    let storage = seed_leaving_a_partial_block(|storage| {
        let (mut wal, _) = open(storage).context("opened")?;
        append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
        assert_eq!(
            append(&mut wal, 3, FULL_BLOCK).context("staged")?,
            Position::new(3)
        );
        storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
        assert_eq!(block_on(wal.sync()), Ok(Position::new(3)));
        assert_eq!(
            append(&mut wal, 4, ONE_BLOCK).context("staged")?,
            Position::new(4)
        );
        storage.fail_at(storage.trace().len(), FaultMode::ShortTornWrite);
        assert_io(&block_on(wal.sync()).err().context("torn short")?, "write");
        Ok(())
    })?;
    let (mut wal, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery.truncation().map(|t| (t.position(), t.offset())),
        Some((Position::new(3), 8_192))
    );
    let bytes = storage.stable_contents(&segment(0)).context("segment 0")?;
    assert_eq!(bytes.len(), 16_384, "cleared to a whole block");
    assert!(bytes[12_288..].iter().all(|b| *b == 0));
    append_rounds(&mut wal, 3, 3, ONE_BLOCK)?;
    Ok(())
}

#[test]
fn test_without_fallocate_a_segment_whose_zeros_were_torn_short_is_filled_at_the_open() -> TestResult
{
    // Segment 0's zeros, written for want of a reservation, torn short inside a block.
    let storage = seed_leaving_a_partial_block(|storage| {
        storage.refuse_preallocation();
        let (mut wal, _) = open(storage).context("opened")?;
        assert_eq!(
            append(&mut wal, 1, ONE_BLOCK).context("staged")?,
            Position::new(1)
        );
        // Created, its reservation refused, then the zeros.
        storage.fail_at(storage.trace().len() + 2, FaultMode::ShortTornWrite);
        assert_io(&block_on(wal.sync()).err().context("torn short")?, "write");
        Ok(())
    })?;
    let (mut wal, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::ZERO);
    assert_eq!(
        storage.stable_contents(&segment(0)).map(|b| b.len()),
        Some(16_384)
    );
    append_rounds(&mut wal, 1, 4, ONE_BLOCK)?;
    drop(wal);
    let (_, recovered) = open(&storage).context("reopened")?;
    assert_eq!(recovered.truncation(), None);
    assert_eq!(recovered.committed(), Position::new(4));
    Ok(())
}

/// What `f` returns, and the fields of every warning it logged.
fn warnings_during<T>(f: impl FnOnce() -> T) -> (T, Vec<BTreeMap<String, String>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture(events.clone()));
    // As in the truncation's warning test: a second dispatcher kept alive and the interest cache rebuilt.
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let value = tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        f()
    });
    let warnings = events
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(level, _)| *level == tracing::Level::WARN)
        .map(|(_, fields)| fields.clone())
        .collect();
    (value, warnings)
}

/// The level and fields of every event, the message under "message".
type Events = Arc<Mutex<Vec<(tracing::Level, BTreeMap<String, String>)>>>;

struct Capture(Events);

struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _previous: Option<String> = self
            .0
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        let _previous: Option<String> = self.0.insert(field.name().to_string(), value.to_string());
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields(BTreeMap::new());
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((*event.metadata().level(), fields.0));
    }
}

/// The short torn writes injected into writes, and the spares taken as new segments, of one seed's run.
fn tally(storage: &SimStorage) -> (usize, usize) {
    let short = storage
        .injected()
        .iter()
        .filter(|(_, op, mode)| {
            *op == StorageOperation::Write && *mode == FaultMode::ShortTornWrite
        })
        .count();
    let taken = storage
        .trace()
        .iter()
        .filter(|(op, path)| {
            *op == StorageOperation::Rename && path.extension().is_some_and(|e| e == "spare")
        })
        .count();
    (short, taken)
}

/// A small generator for the workload's choices, apart from the storage's own seed.
struct Draws(u64);

impl Draws {
    fn next(&mut self, below: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % below
    }
}

/// One seed's run: appends, syncs, checkpoints, restarts and power losses under random faults, checking after every
/// recovery that each acknowledged round above the checkpoint reads back with its checksum, unless a silent torn write
/// was injected and the recovery named the lost round.
fn run_seed(seed: u64) -> TestResult<(usize, usize)> {
    let storage = SimStorage::new(seed);
    let mut draws = Draws(seed ^ 0x9E37_79B9_7F4A_7C15);
    let mut acknowledged: BTreeMap<u64, u64> = BTreeMap::new();
    let mut wal: Option<Wal<SimStorage>> = None;
    for _ in 0..80 {
        let Some(log) = wal.as_mut() else {
            storage.fail_randomly(15);
            let opened = block_on(Wal::open(storage.clone(), dir(), config()));
            storage.fail_randomly(0);
            let (log, recovery) = match opened {
                Ok(opened) => opened,
                Err(LogError(LogErrorEnum::Io { .. })) => continue,
                Err(refusal) => {
                    // Only damage the storage hid can stop a start: a silent tear under an acknowledged round or a
                    // superblock copy.
                    let silent = storage
                        .injected()
                        .iter()
                        .any(|(_, _, mode)| *mode == FaultMode::SilentTornWrite);
                    assert!(
                        silent,
                        "seed {seed}: refused without a silent tear: {refusal:?}"
                    );
                    return Ok(tally(&storage));
                }
            };
            let mut log = log;
            check_recovery(&storage, seed, &mut acknowledged, &recovery, &mut log)?;
            wal = Some(log);
            continue;
        };
        storage.fail_randomly(15);
        match draws.next(10) {
            0 => {
                let committed = log.committed();
                if block_on(log.trim_through(committed)).is_err() {
                    wal = None;
                }
            }
            1 => wal = None,
            2 => {
                wal = None;
                storage.power_loss();
            }
            _ => match append_and_sync(log, &mut draws, seed, &mut acknowledged) {
                AfterAppend::Kept => {}
                AfterAppend::Dropped { power_loss } => {
                    wal = None;
                    if power_loss {
                        storage.power_loss();
                    }
                }
            },
        }
        storage.fail_randomly(0);
    }
    Ok(tally(&storage))
}

/// Checks a recovery of `log` against the rounds `acknowledged`: each lost round was torn silently and the first is
/// named, and every acknowledged round above the checkpoint reads back with its checksum. Forgets the lost rounds.
fn check_recovery(
    storage: &SimStorage,
    seed: u64,
    acknowledged: &mut BTreeMap<u64, u64>,
    recovery: &Recovery,
    log: &mut Wal<SimStorage>,
) -> TestResult {
    let lost: Vec<u64> = acknowledged
        .range(recovery.committed().get() + 1..)
        .map(|(p, _)| *p)
        .collect();
    if let Some(first) = lost.first() {
        // Each lost round was acknowledged, so each was torn silently; the report names the first of them,
        // and the log now ends before it.
        let silent = storage
            .injected()
            .iter()
            .filter(|(_, op, mode)| {
                *mode == FaultMode::SilentTornWrite && *op == StorageOperation::Write
            })
            .count();
        assert!(
            lost.len() <= silent,
            "seed {seed}: acknowledged rounds {lost:?} lost to {silent} silent tears"
        );
        assert_eq!(
            recovery.truncation().map(|t| t.position().get()),
            Some(*first),
            "seed {seed}: the first lost round is named"
        );
    }
    acknowledged.retain(|p, _| *p <= recovery.committed().get());
    let from = recovery.checkpoint().get() + 1;
    // Faults are off: a read of what the recovery found fails only if the log is wrong.
    let rounds = block_on(log.read(Position::new(from), u32::MAX))
        .map_err(|e| format!("seed {seed}: the recovered rounds read back: {e:?}"))
        .context("the recovered rounds read back")?;
    for round in &rounds {
        if let Some(checksum) = acknowledged.get(&round.position().get()) {
            assert_eq!(*checksum, round.checksum(), "seed {seed}");
        }
    }
    let readable: Vec<u64> = rounds.iter().map(|r| r.position().get()).collect();
    let expected: Vec<u64> = acknowledged.range(from..).map(|(p, _)| *p).collect();
    assert!(
        expected.iter().all(|p| readable.contains(p)),
        "seed {seed}: {expected:?} in {readable:?}"
    );
    assert!(
        log.next_event().is_none(),
        "seed {seed}: one node has no events"
    );
    Ok(())
}

/// What became of the log after [`append_and_sync`].
enum AfterAppend {
    /// The log is kept open.
    Kept,
    /// The log failed and is dropped, the power lost after it when `power_loss`.
    Dropped { power_loss: bool },
}

/// Appends the next round to `log` and syncs it, recording it acknowledged once synced. A busy log is trimmed
/// through its committed position instead.
fn append_and_sync(
    log: &mut Wal<SimStorage>,
    draws: &mut Draws,
    seed: u64,
    acknowledged: &mut BTreeMap<u64, u64>,
) -> AfterAppend {
    let position = log.tail().0.get() + 1;
    let len = [10, ONE_BLOCK, TWO_BLOCKS][usize::try_from(draws.next(3)).unwrap_or(usize::MAX)];
    let appended = append(log, position, len);
    assert!(
        matches!(appended, Ok(_) | Err(LogError(LogErrorEnum::Busy { .. }))),
        "seed {seed}: {appended:?}"
    );
    if appended.is_err() {
        let committed = log.committed();
        return match block_on(log.trim_through(committed)) {
            Ok(_) => AfterAppend::Kept,
            Err(_) => AfterAppend::Dropped { power_loss: false },
        };
    }
    match block_on(log.sync()) {
        Ok(committed) => {
            assert_eq!(committed.get(), position);
            let _previous: Option<u64> = acknowledged.insert(position, log.tail().1);
            AfterAppend::Kept
        }
        Err(_) => AfterAppend::Dropped {
            power_loss: draws.next(2) == 0,
        },
    }
}

#[test]
fn test_no_acknowledged_round_is_lost_under_faults_and_power_losses() -> TestResult {
    let (short_tears, spares_taken) = (0..512)
        .map(run_seed)
        .try_fold((0, 0), |(a, b), run| run.map(|(c, d)| (a + c, b + d)))?;
    // The workload reaches a short segment left by a torn extending write, and new segments taken from spares.
    assert!(short_tears > 0, "no short tear");
    assert!(spares_taken > 0, "no spare taken");
    Ok(())
}

#[test]
fn test_a_log_of_another_incarnation_writes_and_recovers_its_rounds_under_it() -> TestResult {
    // A superblock of incarnation 2, as a log restored into a new incarnation holds it.
    let storage = SimStorage::new(1);
    storage
        .put_file(
            &superblock('a'),
            &Superblock::new(1, CLUSTER, 2, Position::ZERO, 0, 0).encode(),
        )
        .context("put")?;
    let (mut wal, _) = open(&storage).context("opened")?;
    assert_eq!(wal.incarnation(), 2);
    let checksums = append_synced(&mut wal, 1, 3, ONE_BLOCK)?;
    drop(wal);
    // The scan accepts only rounds of the superblock's incarnation, so the three rounds came back only if they were
    // written under 2.
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(reopened.incarnation(), 2);
    assert_eq!(recovery.committed(), Position::new(3));
    assert_eq!(reopened.tail(), (Position::new(3), checksums[2]));
    let rounds = block_on(reopened.read(Position::new(1), u32::MAX)).context("read")?;
    assert_eq!(rounds.len(), 3);
    assert_eq!(rounds[2].body(), body(3, ONE_BLOCK).as_slice());
    Ok(())
}

/// A complete round at the stop itself is not past it: round 3, whole and checksummed but naming another parent than
/// round 2, ends the chain as chain damage with nothing complete after it, so it is a torn tail, truncated and
/// reported, and the log continues after round 2.
#[test]
fn test_a_complete_round_breaking_the_chain_at_the_tail_is_truncated() -> TestResult {
    use ultragraphdb_traits_log::RoundRef;
    use ultragraphdb_transactions_log::encode_round;
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    let checksums = append_synced(&mut wal, 1, 2, ONE_BLOCK)?;
    drop(wal);
    let data = body(3, ONE_BLOCK);
    let (frame, _) = encode_round(
        CLUSTER,
        0,
        1,
        &RoundRef::new(Position::new(3), 0xDEAD, 6, 1, derivation_of(3), &data),
    )
    .context("encoded")?;
    // Rounds 1 and 2 take blocks 0 and 1; round 3 is laid in block 2.
    put_segment_0(&storage, 8_192, frame.as_slice())?;
    let (reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery.truncation(),
        Some(Truncation::new(
            Position::new(3),
            0,
            8_192,
            Damage::Chain {
                position: Position::new(3)
            }
        ))
    );
    assert_eq!(recovery.committed(), Position::new(2));
    assert_eq!(reopened.tail(), (Position::new(2), checksums[1]));
    Ok(())
}

/// The first segment kept may start at the checkpoint: a damaged chunk table in its first round, with complete rounds
/// after it, is interior damage named by that round's position, not by the position after the checkpoint.
#[test]
fn test_a_damaged_chunk_table_in_the_first_kept_round_names_that_round() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    // Two-block rounds, two to a four-block segment: rounds 1 and 2 in segment 0, 3 and 4 in 1. (A segment after it
    // starting at round 5 would refuse the start sooner, as a first round past the one after the checkpoint.)
    append_rounds(&mut wal, 1, 4, TWO_BLOCKS)?;
    // The checkpoint covers rounds 1 to 3: segment 0 holds only covered rounds and goes, segment 1 starts at round 3.
    block_on(wal.trim_through(Position::new(3))).context("trimmed")?;
    drop(wal);
    // A 5,000-byte body is two chunks (3,840 and 1,160 bytes) and a 16-byte table after the body: frame bytes 5,256 to
    // 5,272 of round 3, at the start of segment 1.
    storage
        .corrupt(&segment(1), 5_260, 0x01)
        .context("flipped")?;
    let before = storage.trace().len();
    assert_eq!(open(&storage).err(), Some(LogError::corrupt(3, 1, 0)));
    assert_eq!(writes_in(&storage, before), 0, "nothing is truncated");
    Ok(())
}

/// A restart at the checkpoint writes the next superblock: one above the trim's sequence, so to the other copy, with the
/// checkpoint kept and the segment after the stop as the first kept.
#[test]
fn test_a_restart_at_the_checkpoint_writes_the_next_superblock() -> TestResult {
    // The scenario of test_a_torn_round_at_the_checkpoint_after_rounds_below_it_leaves_the_log_at_the_checkpoint:
    // rounds 1 to 3 and the torn round 4 share segment 0, and the checkpoint covers 4.
    let storage = SimStorage::new(9);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 4, FULL_BLOCK).context("staged")?,
        Position::new(4)
    );
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(4)));
    let four = wal.tail().1;
    block_on(wal.trim_through(Position::new(4))).context("trimmed")?;
    drop(wal);
    storage.power_loss();
    let (_, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::new(4));
    // Sequence 1 at creation, 2 for the trim, 3 for the restart. The chain stops at round 4's block in segment 0, so
    // the log continues in segment 1.
    assert_eq!(
        block_on(Superblock::read(&storage, &dir())).context("read")?,
        SuperblockRead::Present(Superblock::new(3, CLUSTER, 1, Position::new(4), four, 1))
    );
    Ok(())
}

/// Without fallocate, a tail segment cut short is filled with zeros at the open from the block its length lies in, so
/// the rounds before it are left as they were and read back.
#[test]
fn test_without_fallocate_a_tail_segment_cut_short_is_filled_after_its_rounds() -> TestResult {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
    drop(wal);
    // Rounds 1 and 2 take blocks 0 and 1; the segment is cut 100 bytes into block 2.
    let mut bytes = storage.contents(&segment(0)).context("segment 0")?;
    bytes.truncate(8_292);
    storage.put_file(&segment(0), &bytes).context("put")?;
    storage.refuse_preallocation();
    let (mut reopened, recovery) = open(&storage).context("reopened")?;
    assert_eq!(recovery.committed(), Position::new(2));
    let stable = storage.stable_contents(&segment(0)).context("segment 0")?;
    assert_eq!(stable.len(), 16_384);
    assert_eq!(stable[..8_192], bytes[..8_192], "the rounds are left");
    assert!(stable[8_192..].iter().all(|b| *b == 0));
    let rounds = block_on(reopened.read(Position::new(1), u32::MAX)).context("read")?;
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[1].body(), body(2, ONE_BLOCK).as_slice());
    Ok(())
}

/// A tail segment of its full length is not reserved again at the open: only one cut short is.
#[test]
fn test_a_full_tail_segment_is_not_reserved_again() -> TestResult {
    // Rounds 1 to 3 and round 4, torn silently, fill segment 0's four blocks, so the chain stops inside it.
    let storage = SimStorage::new(9);
    let (mut wal, _) = open(&storage).context("opened")?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 4, FULL_BLOCK).context("staged")?,
        Position::new(4)
    );
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(4)));
    drop(wal);
    storage.power_loss();
    assert_eq!(storage.contents(&segment(0)).map(|b| b.len()), Some(16_384));
    let before = storage.trace().len();
    let (_, recovery) = open(&storage).context("reopened")?;
    assert_eq!(
        recovery
            .truncation()
            .map(|t| (t.position(), t.segment(), t.offset())),
        Some((Position::new(4), 0, 12_288))
    );
    assert!(
        !storage.trace()[before..]
            .iter()
            .any(|step| *step == (StorageOperation::Preallocate, segment(0))),
        "{:?}",
        &storage.trace()[before..]
    );
    Ok(())
}
