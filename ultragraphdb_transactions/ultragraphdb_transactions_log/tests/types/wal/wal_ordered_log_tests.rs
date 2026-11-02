/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The write-ahead log through the ordered-log interface, on the simulated storage. The test log's segments hold four
//! blocks, so four one-block rounds fill one.

use std::num::NonZeroU32;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use ultragraphdb_error_log::{LogError, LogErrorEnum};
use ultragraphdb_protocol_wire::HeaderView;
use ultragraphdb_traits_log::{OrderedLog, Position, RoundRef};
use ultragraphdb_transactions_log::{
    CompioStorage, FaultMode, SimStorage, StorageOperation, Superblock, SuperblockRead, Wal,
    encode_round,
};

use crate::utils::helpers::{
    CLUSTER, ONE_BLOCK, TWO_BLOCKS, TempDir, append, append_rounds, append_synced, block_on, body,
    config, dir, frame_checksum, open, run, segment, segment_in, spare, superblock,
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

fn fresh() -> TestResult<(SimStorage, Wal<SimStorage>)> {
    let storage = SimStorage::new(1);
    let (wal, _) = open(&storage).context("opened")?;
    Ok((storage, wal))
}

#[test]
fn test_an_append_out_of_order_is_refused_naming_the_next_position() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 5, ONE_BLOCK)?;
    let tail = wal.tail();
    let before = storage.trace().len();
    for (position, found) in [(7, 7), (5, 5), (0, 0), (u64::MAX, u64::MAX)] {
        let data = body(position, ONE_BLOCK);
        let refused = wal
            .append(RoundRef::new(Position::new(position), tail.1, 0, 1, &data))
            .err()
            .context("out of order")?;
        assert_eq!(refused, LogError::PositionMismatch(6, found));
    }
    assert_eq!(wal.tail(), tail);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(5)));
    assert_eq!(storage.trace().len(), before, "nothing is written");
    Ok(())
}

#[test]
fn test_an_append_with_another_parent_is_refused() -> TestResult {
    let (_, mut wal) = fresh()?;
    let checksums = append_synced(&mut wal, 1, 2, ONE_BLOCK)?;
    let data = body(3, ONE_BLOCK);
    let refused = wal
        .append(RoundRef::new(Position::new(3), checksums[0], 6, 1, &data))
        .err()
        .context("not the tail's checksum")?;
    assert_eq!(
        refused,
        LogError::ParentMismatch(3, checksums[1], checksums[0])
    );
    // A fresh log's first round names the checkpoint's checksum, 0.
    let (_, mut fresh) = fresh()?;
    let refused_first = fresh
        .append(RoundRef::new(Position::new(1), 1, 2, 1, &data))
        .err()
        .context("not 0")?;
    assert_eq!(refused_first, LogError::ParentMismatch(1, 0, 1));
    Ok(())
}

#[test]
fn test_an_append_stages_and_writes_nothing() -> TestResult {
    let (storage, mut wal) = fresh()?;
    let before = storage.trace().len();
    assert_eq!(append(&mut wal, 1, ONE_BLOCK), Ok(Position::new(1)));
    assert_eq!(storage.trace().len(), before);
    assert_eq!(wal.committed(), Position::ZERO);
    assert_eq!(wal.tail().0, Position::new(1));
    assert_eq!(block_on(wal.sync()), Ok(Position::new(1)));
    assert_eq!(wal.committed(), Position::new(1));
    Ok(())
}

#[test]
fn test_each_round_is_one_write_and_one_sync_after_its_new_segment() -> TestResult {
    let (storage, mut wal) = fresh()?;
    let before = storage.trace().len();
    for position in 1..=3 {
        assert_eq!(
            append(&mut wal, position, ONE_BLOCK).context("accepted")?,
            Position::new(position)
        );
    }
    assert_eq!(block_on(wal.sync()), Ok(Position::new(3)));
    let seg = segment(0);
    let steps: Vec<_> = storage.trace()[before..].to_vec();
    assert_eq!(
        steps,
        vec![
            (StorageOperation::Create, seg.clone()),
            (StorageOperation::Preallocate, seg.clone()),
            (StorageOperation::FileSync, seg.clone()),
            (StorageOperation::DirectorySync, dir()),
            (StorageOperation::Write, seg.clone()),
            (StorageOperation::FileSync, seg.clone()),
            (StorageOperation::Write, seg.clone()),
            (StorageOperation::FileSync, seg.clone()),
            (StorageOperation::Write, seg.clone()),
            (StorageOperation::FileSync, seg),
        ]
    );
    let before_idle_sync = storage.trace().len();
    assert_eq!(block_on(wal.sync()), Ok(Position::new(3)));
    assert_eq!(
        storage.trace().len(),
        before_idle_sync,
        "a sync with nothing staged does nothing"
    );
    Ok(())
}

#[test]
fn test_rounds_lie_at_block_boundaries_padded_with_zeros() -> TestResult {
    let (storage, mut wal) = fresh()?;
    let checksums = append_synced(&mut wal, 1, 2, ONE_BLOCK)?;
    let bytes = storage
        .stable_contents(&segment(0))
        .context("the segment")?;
    // A segment's blocks are reserved, not written: it is as long as its rounds.
    assert_eq!(bytes.len(), 8_192);
    assert_eq!(&bytes[256..1_256], body(1, ONE_BLOCK).as_slice());
    assert!(bytes[1_256..4_096].iter().all(|b| *b == 0));
    assert_eq!(
        &bytes[4_096 + 256..4_096 + 1_256],
        body(2, ONE_BLOCK).as_slice()
    );
    assert_eq!(frame_checksum(&bytes[..4_096]), checksums[0]);
    assert_eq!(frame_checksum(&bytes[4_096..]), checksums[1]);
    assert_eq!(wal.tail(), (Position::new(2), checksums[1]));
    Ok(())
}

#[test]
fn test_a_round_never_spans_two_segments() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    // Round 4 takes two blocks; one is left in segment 0, so it starts segment 1.
    append_rounds(&mut wal, 4, 4, TWO_BLOCKS)?;
    let first = storage.stable_contents(&segment(0)).context("segment 0")?;
    assert!(first[12_288..].iter().all(|b| *b == 0));
    let second = storage.stable_contents(&segment(1)).context("segment 1")?;
    assert_eq!(&second[256..5_256], body(4, TWO_BLOCKS).as_slice());
    assert_eq!(frame_checksum(&second), wal.tail().1);
    Ok(())
}

#[test]
fn test_a_round_too_large_for_a_frame_or_a_segment_is_refused() -> TestResult {
    let (storage, mut wal) = fresh()?;
    let before = storage.trace().len();
    let past_frame = vec![1u8; 1_046_273];
    let refused = wal
        .append(RoundRef::new(Position::new(1), 0, 2, 1, &past_frame))
        .err()
        .context("past the frame")?;
    assert_eq!(refused, LogError::TooLarge(1_048_577, 1_048_576));
    // 20,000 bytes take five chunks and a 40-byte table: a 20,296-byte frame, five blocks; the segment holds four.
    let past_segment = vec![1u8; 20_000];
    let refused_segment = wal
        .append(RoundRef::new(Position::new(1), 0, 2, 1, &past_segment))
        .err()
        .context("past the segment")?;
    assert_eq!(refused_segment, LogError::TooLarge(20_480, 16_384));
    // A round of a whole segment fits: 16,000 bytes take four chunks and a 32-byte table, 16,288 bytes, four blocks.
    assert_eq!(wal.tail(), (Position::ZERO, 0));
    assert_eq!(storage.trace().len(), before);
    let whole = vec![1u8; 16_000];
    assert_eq!(
        wal.append(RoundRef::new(Position::new(1), 0, 2, 1, &whole)),
        Ok(Position::new(1))
    );
    Ok(())
}

#[test]
fn test_an_append_past_the_capacity_is_busy_until_a_checkpoint_trims() -> TestResult {
    let storage = SimStorage::new(1);
    let small = config().with_capacity(NonZeroU32::new(2).context("nonzero")?);
    let (mut wal, _) = block_on(Wal::open(storage.clone(), dir(), small)).context("opened")?;
    append_rounds(&mut wal, 1, 8, ONE_BLOCK)?;
    let before = storage.trace().len();
    assert_eq!(append(&mut wal, 9, ONE_BLOCK), Err(LogError::Busy(2)));
    assert_eq!(storage.trace().len(), before);
    block_on(wal.trim_through(Position::new(4), 0)).context("trimmed")?;
    assert_eq!(append(&mut wal, 9, ONE_BLOCK), Ok(Position::new(9)));
    assert_eq!(block_on(wal.sync()), Ok(Position::new(9)));
    Ok(())
}

#[test]
fn test_a_sealed_log_refuses_appends_and_reports_its_tail() -> TestResult {
    let (_, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 8, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 9, ONE_BLOCK).context("staged")?,
        Position::new(9)
    );
    assert_eq!(wal.seal(), Ok(Position::new(9)));
    assert_eq!(append(&mut wal, 10, ONE_BLOCK), Err(LogError::Sealed(9)));
    assert_eq!(wal.seal(), Ok(Position::new(9)));
    // Sealing stops appends, not the sync of what was staged.
    assert_eq!(block_on(wal.sync()), Ok(Position::new(9)));
    Ok(())
}

#[test]
fn test_a_failed_write_or_sync_poisons_the_log() -> TestResult {
    // Each operation of a first round's sync: the segment's creation, preallocation, sync and directory sync, the
    // round's write and its sync.
    let operations = [
        "create",
        "preallocate",
        "file sync",
        "directory sync",
        "write",
        "file sync",
    ];
    for (step, &operation) in operations.iter().enumerate() {
        let (storage, mut wal) = fresh()?;
        assert_eq!(
            append(&mut wal, 1, ONE_BLOCK).context("staged")?,
            Position::new(1)
        );
        storage.fail_at(storage.trace().len() + step, FaultMode::Before);
        let error = block_on(wal.sync()).err().context("faulted")?;
        assert_io(&error, operation);
        assert_eq!(wal.committed(), Position::ZERO);
        assert_eq!(append(&mut wal, 2, ONE_BLOCK), Err(LogError::Poisoned()));
        assert_eq!(block_on(wal.sync()), Err(LogError::Poisoned()));
        assert_eq!(
            block_on(wal.read(Position::new(1), 1)),
            Err(LogError::Poisoned())
        );
        assert_eq!(
            block_on(wal.trim_through(Position::ZERO, 0)),
            Err(LogError::Poisoned())
        );
        assert_eq!(wal.seal(), Err(LogError::Poisoned()));
    }
    Ok(())
}

#[test]
fn test_io_errors_name_the_operation() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 1, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 2, ONE_BLOCK).context("staged")?,
        Position::new(2)
    );
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let error = block_on(wal.sync()).err().context("faulted")?;
    assert!(
        matches!(
            error.0,
            LogErrorEnum::Io {
                operation: "write",
                ..
            }
        ),
        "{error:?}"
    );
    let (sync_storage, mut sync_wal) = fresh()?;
    append_rounds(&mut sync_wal, 1, 1, ONE_BLOCK)?;
    assert_eq!(
        append(&mut sync_wal, 2, ONE_BLOCK).context("staged")?,
        Position::new(2)
    );
    sync_storage.fail_at(sync_storage.trace().len() + 1, FaultMode::Before);
    let sync_error = block_on(sync_wal.sync()).err().context("faulted")?;
    assert!(
        matches!(
            sync_error.0,
            LogErrorEnum::Io {
                operation: "file sync",
                ..
            }
        ),
        "{sync_error:?}"
    );
    Ok(())
}

#[test]
fn test_read_returns_the_rounds_as_appended_and_verified() -> TestResult {
    let (_, mut wal) = fresh()?;
    let checksums = append_synced(&mut wal, 1, 5, ONE_BLOCK)?;
    let rounds = block_on(wal.read(Position::new(1), u32::MAX)).context("read")?;
    assert_eq!(rounds.len(), 5);
    for (i, round) in rounds.iter().enumerate() {
        let position = i as u64 + 1;
        assert_eq!(round.position(), Position::new(position));
        assert_eq!(round.checksum(), checksums[i]);
        assert_eq!(round.parent(), if i == 0 { 0 } else { checksums[i - 1] });
        assert_eq!(round.first_version(), 2 * position);
        assert_eq!(round.transactions(), 1);
        assert_eq!(round.body(), body(position, ONE_BLOCK).as_slice());
    }
    let from_three = block_on(wal.read(Position::new(3), u32::MAX)).context("read")?;
    let positions: Vec<u64> = from_three.iter().map(|r| r.position().get()).collect();
    assert_eq!(positions, [3, 4, 5]);
    Ok(())
}

#[test]
fn test_read_is_bounded_by_bytes_of_frames_and_returns_at_least_one() -> TestResult {
    let (_, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 5, ONE_BLOCK)?;
    // Each frame is 1,256 bytes.
    let mut count = |max| -> TestResult<usize> {
        Ok(block_on(wal.read(Position::new(1), max))
            .context("read")?
            .len())
    };
    assert_eq!(count(0)?, 1);
    assert_eq!(count(1)?, 1);
    assert_eq!(count(2_511)?, 1);
    assert_eq!(count(2_512)?, 2);
    assert_eq!(count(6_280)?, 5);
    Ok(())
}

#[test]
fn test_read_takes_positions_from_the_first_kept_to_the_one_after_committed() -> TestResult {
    let (_, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 5, ONE_BLOCK)?;
    assert_eq!(
        append(&mut wal, 6, ONE_BLOCK).context("staged")?,
        Position::new(6)
    );
    assert_eq!(
        block_on(wal.read(Position::new(6), u32::MAX)),
        Ok(Vec::new())
    );
    assert_eq!(
        block_on(wal.read(Position::new(1), u32::MAX))
            .context("read")?
            .len(),
        5
    );
    assert_eq!(
        block_on(wal.read(Position::new(7), u32::MAX)),
        Err(LogError::PositionMismatch(6, 7))
    );
    assert_eq!(
        block_on(wal.read(Position::ZERO, u32::MAX)),
        Err(LogError::PositionMismatch(1, 0))
    );
    let (_, mut empty) = fresh()?;
    assert_eq!(
        block_on(empty.read(Position::new(1), u32::MAX)),
        Ok(Vec::new())
    );
    Ok(())
}

#[test]
fn test_read_refuses_a_round_that_no_longer_verifies() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    storage
        .corrupt(&segment(0), 4_096 + 256 + 10, 0x01)
        .context("flipped")?;
    assert_eq!(
        block_on(wal.read(Position::new(1), u32::MAX)),
        Err(LogError::CorruptChunk(2, 0, 4_096, 0, 0, 1_000))
    );
    assert_eq!(
        block_on(wal.read(Position::new(3), u32::MAX))
            .context("read")?
            .len(),
        1
    );
    storage
        .corrupt(&segment(0), 8_192 + 60, 0x01)
        .context("flipped")?;
    assert_eq!(
        block_on(wal.read(Position::new(3), u32::MAX)),
        Err(LogError::Corrupt(3, 0, 8_192))
    );
    Ok(())
}

#[test]
fn test_read_refuses_a_valid_frame_that_is_not_the_round_logged_there() -> TestResult {
    // Round 2's block is overwritten in place by a valid frame of round 2 with another body: every checksum in the
    // frame holds, but it is not the round the log wrote there.
    let (storage, mut wal) = fresh()?;
    let checksums = append_synced(&mut wal, 1, 3, ONE_BLOCK)?;
    let other = body(99, ONE_BLOCK);
    let (frame, _) = encode_round(
        CLUSTER,
        0,
        1,
        &RoundRef::new(Position::new(2), checksums[0], 4, 1, &other),
    )
    .context("encoded")?;
    let old = storage.contents(&segment(0)).context("segment 0")?;
    for (i, (was, now)) in old[4_096..8_192].iter().zip(frame.as_slice()).enumerate() {
        if was != now {
            storage
                .corrupt(&segment(0), 4_096 + i, was ^ now)
                .context("rewritten")?;
        }
    }
    assert_eq!(
        block_on(wal.read(Position::new(2), u32::MAX)),
        Err(LogError::Corrupt(2, 0, 4_096))
    );
    Ok(())
}

#[test]
fn test_trim_through_records_the_checkpoint_and_removes_whole_segments() -> TestResult {
    let (storage, mut wal) = fresh()?;
    let checksums = append_synced(&mut wal, 1, 10, ONE_BLOCK)?;
    assert_eq!(
        block_on(wal.trim_through(Position::new(11), 0)),
        Err(LogError::PositionMismatch(10, 11))
    );
    block_on(wal.trim_through(Position::new(8), 0)).context("trimmed")?;
    assert_eq!(wal.checkpoint(), Position::new(8));
    // Segments 0 and 1 hold rounds 1 to 8; segment 2 holds 9 and 10.
    assert_eq!(storage.contents(&segment(0)), None);
    assert_eq!(storage.contents(&segment(1)), None);
    assert_eq!(
        storage.stable_contents(&segment(0)),
        None,
        "the removals are durable"
    );
    assert!(storage.contents(&segment(2)).is_some());
    let read = block_on(Superblock::read(&storage, &dir())).context("read")?;
    assert!(
        matches!(read, SuperblockRead::Present(_)),
        "a superblock: {read:?}"
    );
    let SuperblockRead::Present(record) = read else {
        return Err(TestError::new("a superblock"));
    };
    assert_eq!(record.checkpoint(), Position::new(8));
    assert_eq!(record.checkpoint_checksum(), checksums[7]);
    assert_eq!(record.first_segment(), 2);
    assert_eq!(record.sequence(), 2);
    assert!(storage.stable_contents(&superblock('b')).is_some());
    assert_eq!(
        block_on(wal.read(Position::new(8), u32::MAX)),
        Err(LogError::PositionMismatch(9, 8))
    );
    assert_eq!(
        block_on(wal.read(Position::new(9), u32::MAX))
            .context("read")?
            .len(),
        2
    );
    Ok(())
}

#[test]
fn test_a_failed_trim_poisons_the_log() -> TestResult {
    // Each step of a checkpoint: the superblock's temporary created, written, synced, renamed, its directory synced,
    // then segment 0 zeroed, synced and renamed a spare, and the directory's sync.
    let operations = [
        "create",
        "write",
        "file sync",
        "rename",
        "directory sync",
        "write",
        "file sync",
        "rename",
        "directory sync",
    ];
    for (step, &operation) in operations.iter().enumerate() {
        let (storage, mut wal) = fresh()?;
        append_rounds(&mut wal, 1, 6, ONE_BLOCK)?;
        storage.fail_at(storage.trace().len() + step, FaultMode::Before);
        let error = block_on(wal.trim_through(Position::new(5), 0))
            .err()
            .context("faulted")?;
        assert_io(&error, operation);
        assert_eq!(
            append(&mut wal, 7, ONE_BLOCK),
            Err(LogError::Poisoned()),
            "step {step}"
        );
        assert_eq!(
            block_on(wal.trim_through(Position::new(6), 0)),
            Err(LogError::Poisoned())
        );
    }
    Ok(())
}

#[test]
fn test_trim_through_at_or_below_the_checkpoint_changes_nothing() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 6, ONE_BLOCK)?;
    block_on(wal.trim_through(Position::new(5), 0)).context("trimmed")?;
    let before = storage.trace().len();
    block_on(wal.trim_through(Position::new(5), 0)).context("no change")?;
    block_on(wal.trim_through(Position::new(2), 0)).context("no change")?;
    block_on(wal.trim_through(Position::ZERO, 0)).context("no change")?;
    assert_eq!(storage.trace().len(), before);
    assert_eq!(wal.checkpoint(), Position::new(5));
    Ok(())
}

#[test]
fn test_trim_through_keeps_the_segment_the_next_round_goes_to() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 4, ONE_BLOCK)?;
    block_on(wal.trim_through(Position::new(4), 0)).context("trimmed")?;
    assert!(storage.contents(&segment(0)).is_some());
    append_rounds(&mut wal, 5, 5, ONE_BLOCK)?;
    assert_eq!(wal.committed(), Position::new(5));
    Ok(())
}

#[test]
fn test_the_local_log_has_no_events() -> TestResult {
    let (_, mut wal) = fresh()?;
    assert_eq!(wal.next_event(), None);
    append_rounds(&mut wal, 1, 2, ONE_BLOCK)?;
    assert_eq!(wal.next_event(), None);
    Ok(())
}

#[test]
fn test_the_log_on_compio_survives_a_reopen() -> TestResult {
    let tmp = TempDir::new("wal")?;
    let path = tmp.path().join("log");
    run(async {
        let (mut wal, recovery) = Wal::open(CompioStorage, &path, config())
            .await
            .context("opened")?;
        assert_eq!(recovery.committed(), Position::ZERO);
        for position in 1..=6u64 {
            let parent = wal.tail().1;
            let data = body(position, if position == 4 { TWO_BLOCKS } else { ONE_BLOCK });
            let _appended: Position = wal
                .append(RoundRef::new(
                    Position::new(position),
                    parent,
                    2 * position,
                    1,
                    &data,
                ))
                .context("accepted")?;
            assert_eq!(wal.sync().await, Ok(Position::new(position)));
        }
        TestResult::Ok(())
    })??;
    // Rounds 1 to 3 and the two-block round 4 do not fit one four-block segment: 4 starts segment 1. Each segment is
    // as long as its rounds, never written with zeros.
    assert_eq!(
        std::fs::metadata(segment_in(&path, 0))
            .context("segment 0")?
            .len(),
        12_288
    );
    assert_eq!(
        std::fs::metadata(segment_in(&path, 1))
            .context("segment 1")?
            .len(),
        16_384
    );
    run(async {
        let (mut wal, recovery) = Wal::open(CompioStorage, &path, config())
            .await
            .context("reopened")?;
        assert_eq!(recovery.committed(), Position::new(6));
        assert_eq!(recovery.truncation(), None);
        let rounds = wal.read(Position::new(1), u32::MAX).await.context("read")?;
        assert_eq!(rounds.len(), 6);
        assert_eq!(rounds[3].body(), body(4, TWO_BLOCKS).as_slice());
        assert_eq!(wal.incarnation(), 1);
        assert_eq!(wal.config().cluster(), CLUSTER);
        TestResult::Ok(())
    })??;
    Ok(())
}

#[test]
fn test_a_checkpoint_zeroes_its_trimmed_segments_and_renames_them_spares() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 12, ONE_BLOCK)?;
    let before = storage.trace().len();
    // Rounds 1 to 8 fill segments 0 and 1; the checkpoint at 8 trims both.
    block_on(wal.trim_through(Position::new(8), 0)).context("trimmed")?;
    // After the superblock's five steps: each segment zeroed whole, synced, renamed; then the directory synced.
    assert_eq!(
        storage.trace()[before + 5..],
        [
            (StorageOperation::Write, segment(0)),
            (StorageOperation::FileSync, segment(0)),
            (StorageOperation::Rename, segment(0)),
            (StorageOperation::Write, segment(1)),
            (StorageOperation::FileSync, segment(1)),
            (StorageOperation::Rename, segment(1)),
            (StorageOperation::DirectorySync, dir()),
        ]
    );
    for index in [0, 1] {
        assert_eq!(storage.stable_contents(&segment(index)), None);
        assert_eq!(
            storage.stable_contents(&spare(index)),
            Some(vec![0u8; 16_384])
        );
    }
    assert!(storage.stable_contents(&segment(2)).is_some());
    Ok(())
}

#[test]
fn test_a_new_segment_takes_a_spare_and_allocates_nothing() -> TestResult {
    let (storage, mut wal) = fresh()?;
    append_rounds(&mut wal, 1, 12, ONE_BLOCK)?;
    block_on(wal.trim_through(Position::new(8), 0)).context("trimmed")?;
    let before = storage.trace().len();
    append_rounds(&mut wal, 13, 13, ONE_BLOCK)?;
    // The lowest spare renamed as segment 3 and its entry made durable before the round's one write and one sync;
    // nothing is created or reserved, and the write lies within the segment's length.
    assert_eq!(
        storage.trace()[before..],
        [
            (StorageOperation::Rename, spare(0)),
            (StorageOperation::DirectorySync, dir()),
            (StorageOperation::Open, segment(3)),
            (StorageOperation::Write, segment(3)),
            (StorageOperation::FileSync, segment(3)),
        ]
    );
    assert_eq!(storage.stable_contents(&spare(0)), None);
    assert!(storage.stable_contents(&spare(1)).is_some());
    let bytes = storage.stable_contents(&segment(3)).context("segment 3")?;
    assert_eq!(bytes.len(), 16_384);
    assert_eq!(&bytes[256..1_256], body(13, ONE_BLOCK).as_slice());
    assert!(bytes[4_096..].iter().all(|b| *b == 0));
    // The next new segment takes the other spare, and the one after it is created.
    append_rounds(&mut wal, 14, 17, ONE_BLOCK)?;
    assert_eq!(storage.stable_contents(&spare(1)), None);
    let before_created = storage.trace().len();
    append_rounds(&mut wal, 18, 21, ONE_BLOCK)?;
    assert!(
        storage.trace()[before_created..].contains(&(StorageOperation::Create, segment(5))),
        "no spare is left"
    );
    Ok(())
}

/// A checkpoint records the last commit version it covers beside its position, durably: the log reopened reads it
/// back, and a trim at or below the checkpoint changes neither.
#[test]
fn test_a_checkpoint_records_the_version_it_covers() -> TestResult {
    let (storage, mut wal) = fresh()?;
    assert_eq!(wal.checkpoint_version(), 0, "before the first checkpoint");
    append_rounds(&mut wal, 1, 3, ONE_BLOCK)?;
    block_on(wal.trim_through(Position::new(2), 40)).context("trimmed")?;
    assert_eq!(wal.checkpoint_version(), 40);
    block_on(wal.trim_through(Position::new(1), 99)).context("no change")?;
    assert_eq!(
        wal.checkpoint_version(),
        40,
        "a trim below the checkpoint changes nothing"
    );
    drop(wal);
    let (reopened, _) = open(&storage).context("reopened")?;
    assert_eq!(reopened.checkpoint_version(), 40);
    assert_eq!(reopened.checkpoint(), Position::new(2));
    Ok(())
}

/// The log writes its configured release into each Round frame's header (`PROTOCOL.md` §12, point 18): the value read
/// back by the wire crate's header view, not the default 0.
#[test]
fn test_each_round_names_the_release_the_log_is_configured_with() -> TestResult {
    let storage = SimStorage::new(1);
    let released = config().with_release(0x0006_0001);
    let (mut wal, _) = block_on(Wal::open(storage.clone(), dir(), released)).context("opened")?;
    let checksums = append_synced(&mut wal, 1, 2, ONE_BLOCK)?;
    let bytes = storage.stable_contents(&segment(0)).context("segment 0")?;
    for (i, block) in bytes.chunks(4_096).enumerate() {
        let header = HeaderView::new(block).context("a header")?;
        assert_eq!(header.release(), 0x0006_0001, "round {}", i + 1);
        // The release is inside the checksummed header: the checksum computed independently still matches.
        assert_eq!(frame_checksum(block), checksums[i]);
    }
    Ok(())
}

/// A log whose tail is the last position, `u64::MAX`, has exhausted its positions: no round can follow it, so every
/// append is refused as `Exhausted`, naming that position, whatever position or parent it names, and the positions never
/// wrap to 0. The rounds at `u64::MAX - 1` and `u64::MAX` themselves are accepted, written and recovered.
#[test]
fn test_the_last_position_ends_the_log_without_wrapping() -> TestResult {
    let storage = SimStorage::new(1);
    // A checkpoint two below the last position, with checksum 0x77; no segment yet.
    storage
        .put_file(
            &superblock('a'),
            &Superblock::new(1, CLUSTER, 1, Position::new(u64::MAX - 2), 0x77, 0, 0).encode(),
        )
        .context("put")?;
    let (mut wal, recovery) = open(&storage).context("opened")?;
    assert_eq!(recovery.committed(), Position::new(u64::MAX - 2));
    assert_eq!(wal.tail(), (Position::new(u64::MAX - 2), 0x77));
    let data = body(1, ONE_BLOCK);
    let next_to_last = Position::new(u64::MAX - 1);
    assert_eq!(
        wal.append(RoundRef::new(next_to_last, 0x77, 0, 1, &data)),
        Ok(next_to_last)
    );
    let parent = wal.tail().1;
    assert_eq!(
        wal.append(RoundRef::new(Position::new(u64::MAX), parent, 0, 1, &data)),
        Ok(Position::new(u64::MAX))
    );
    assert_eq!(block_on(wal.sync()), Ok(Position::new(u64::MAX)));
    let tail = wal.tail();
    let before = storage.trace().len();
    // Position 0 is what u64::MAX + 1 would wrap to; u64::MAX is the tail's own; a wrong parent and a body past the
    // frame would each be refused otherwise, but exhaustion is decided first.
    let past_frame = vec![1u8; 1_046_273];
    for (position, named_parent, bytes) in [
        (0, tail.1, data.as_slice()),
        (u64::MAX, tail.1, data.as_slice()),
        (0, tail.1 ^ 1, data.as_slice()),
        (0, tail.1, past_frame.as_slice()),
    ] {
        assert_eq!(
            wal.append(RoundRef::new(
                Position::new(position),
                named_parent,
                0,
                1,
                bytes
            )),
            Err(LogError::Exhausted(u64::MAX)),
            "position {position}"
        );
    }
    assert_eq!(wal.tail(), tail);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(u64::MAX)));
    assert_eq!(storage.trace().len(), before, "nothing is written");
    // A sealed log is refused as sealed first, as the order of the refusals says.
    assert_eq!(wal.seal(), Ok(Position::new(u64::MAX)));
    assert_eq!(
        wal.append(RoundRef::new(Position::ZERO, tail.1, 0, 1, &data)),
        Err(LogError::Sealed(u64::MAX))
    );
    drop(wal);
    let (mut reopened, recovered) = open(&storage).context("reopened")?;
    assert_eq!(
        (recovered.committed(), recovered.truncation()),
        (Position::new(u64::MAX), None)
    );
    let rounds = block_on(reopened.read(Position::new(u64::MAX), u32::MAX)).context("read")?;
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].checksum(), tail.1);
    assert_eq!(rounds[0].body(), data.as_slice());
    // Reopened, the log is exhausted still.
    assert_eq!(
        reopened.append(RoundRef::new(Position::ZERO, tail.1, 0, 1, &data)),
        Err(LogError::Exhausted(u64::MAX))
    );
    Ok(())
}

/// A checkpoint at the last position covers the round there, and no round follows it: the log reopens with that round
/// first in its first kept segment, committed and checkpointed at `u64::MAX`, and is exhausted, never diverged.
#[test]
fn test_a_checkpoint_at_the_last_position_reopens() -> TestResult {
    let storage = SimStorage::new(1);
    storage
        .put_file(
            &superblock('a'),
            &Superblock::new(1, CLUSTER, 1, Position::new(u64::MAX - 1), 0x77, 0, 0).encode(),
        )
        .context("put")?;
    let (mut wal, _) = open(&storage).context("opened")?;
    let data = body(1, ONE_BLOCK);
    // Round u64::MAX starts segment 0, so the first kept segment's first round is the checkpoint's.
    let _appended: Position = wal
        .append(RoundRef::new(Position::new(u64::MAX), 0x77, 0, 1, &data))
        .context("accepted")?;
    assert_eq!(block_on(wal.sync()), Ok(Position::new(u64::MAX)));
    let last = wal.tail().1;
    block_on(wal.trim_through(Position::new(u64::MAX), 90)).context("trimmed")?;
    assert_eq!(wal.checkpoint(), Position::new(u64::MAX));
    drop(wal);
    let (mut reopened, recovery) = open(&storage).context("reopened, not diverged")?;
    assert_eq!(
        (
            recovery.checkpoint(),
            recovery.committed(),
            recovery.truncation()
        ),
        (Position::new(u64::MAX), Position::new(u64::MAX), None)
    );
    assert_eq!(reopened.tail(), (Position::new(u64::MAX), last));
    assert_eq!(reopened.checkpoint_version(), 90);
    assert_eq!(
        reopened.append(RoundRef::new(Position::ZERO, last, 0, 1, &data)),
        Err(LogError::Exhausted(u64::MAX))
    );
    Ok(())
}
