/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Opening the write-ahead log: a fresh log, the superblock's outcomes, and the recovery's scan of the segments with
//! its corruption rules; then the log under the simulated storage's random faults and power losses.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, SubscriberExt};
use ultragraphdb_error_log::{LogError, LogErrorEnum};
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_transactions_log::{
    Damage, DurableStorage, FaultMode, SimStorage, StorageOperation, Superblock, SuperblockRead,
    Truncation, Wal,
};

use crate::utils::helpers::{
    CLUSTER, FULL_BLOCK, FULL_TWO_BLOCKS, ONE_BLOCK, TWO_BLOCKS, append, append_synced, block_on,
    body, config, dir, open, segment, superblock,
};

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
fn test_a_fresh_log_writes_its_superblock_durably() {
    let storage = SimStorage::new(1);
    let (wal, recovery) = open(&storage).expect("opened");
    assert_eq!(recovery.checkpoint(), Position::ZERO);
    assert_eq!(recovery.committed(), Position::ZERO);
    assert_eq!(recovery.truncation(), None);
    assert_eq!(wal.committed(), Position::ZERO);
    assert_eq!(wal.tail(), (Position::ZERO, 0));
    assert_eq!(wal.incarnation(), 1);
    assert_eq!(wal.checkpoint(), Position::ZERO);
    storage.power_loss();
    assert_eq!(
        block_on(Superblock::read(&storage, &dir())).expect("read"),
        SuperblockRead::Present(Superblock::new(1, CLUSTER, 1, Position::ZERO, 0, 0))
    );
    // Opened again, the log is the same log: no second superblock is written.
    let before = storage.trace().len();
    let (wal, _) = open(&storage).expect("reopened");
    assert_eq!(wal.incarnation(), 1);
    assert!(
        !storage.trace()[before..]
            .iter()
            .any(|(op, _)| *op == StorageOperation::Rename)
    );
}

#[test]
fn test_reopening_recovers_the_synced_rounds_and_drops_the_staged() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    let checksums = append_synced(&mut wal, 1, 5, ONE_BLOCK);
    append(&mut wal, 6, ONE_BLOCK).expect("staged");
    drop(wal);
    let (wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.committed(), Position::new(5));
    assert_eq!(recovery.truncation(), None);
    assert_eq!(wal.tail(), (Position::new(5), checksums[4]));
    assert_eq!(wal.committed(), Position::new(5));
}

#[test]
fn test_a_corrupt_superblock_copy_refuses_the_start_and_writes_nothing() {
    for copy in ['a', 'b'] {
        let storage = SimStorage::new(1);
        let (mut wal, _) = open(&storage).expect("opened");
        append_synced(&mut wal, 1, 6, ONE_BLOCK);
        block_on(wal.trim_through(Position::new(2))).expect("copy b written");
        drop(wal);
        storage
            .corrupt(&superblock(copy), 20, 0x04)
            .expect("flipped");
        let before = storage.trace().len();
        assert_eq!(
            open(&storage).err(),
            Some(LogError::SuperblockUnreadable(copy))
        );
        assert_eq!(writes_in(&storage, before), 0, "copy {copy}");
    }
}

#[test]
fn test_a_superblock_of_another_cluster_diverges() {
    let storage = SimStorage::new(1);
    storage
        .put_file(
            &superblock('a'),
            &Superblock::new(1, CLUSTER ^ 1, 1, Position::ZERO, 0, 0).encode(),
        )
        .expect("put");
    assert_eq!(
        open(&storage).err(),
        Some(LogError::Diverged("the cluster", CLUSTER, CLUSTER ^ 1))
    );
}

#[test]
fn test_segments_without_a_superblock_are_refused_not_overwritten() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 6, ONE_BLOCK);
    drop(wal);
    for copy in ['a', 'b'] {
        let _ = block_on(storage.remove_file(&superblock(copy)));
    }
    let before = storage.trace().len();
    assert_eq!(
        open(&storage).err(),
        Some(LogError::Diverged("segments without a superblock", 0, 2))
    );
    assert_eq!(writes_in(&storage, before), 0);
}

#[test]
fn test_open_finishes_an_interrupted_trim_and_sweeps_temporaries() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    let checksums = append_synced(&mut wal, 1, 10, ONE_BLOCK);
    drop(wal);
    // The checkpoint at 8 was recorded, and the power was lost before segments 0 and 1 were removed.
    storage
        .put_file(
            &superblock('b'),
            &Superblock::new(2, CLUSTER, 1, Position::new(8), checksums[7], 2).encode(),
        )
        .expect("put");
    storage
        .put_file(&dir().join("superblock.a.tmp"), b"half")
        .expect("put");
    let (mut wal, recovery) = open(&storage).expect("opened");
    assert_eq!(recovery.checkpoint(), Position::new(8));
    assert_eq!(recovery.committed(), Position::new(10));
    assert_eq!(storage.stable_contents(&segment(0)), None);
    assert_eq!(storage.stable_contents(&segment(1)), None);
    assert_eq!(
        storage.stable_contents(&dir().join("superblock.a.tmp")),
        None
    );
    let rounds = block_on(wal.read(Position::new(9), u32::MAX)).expect("read");
    assert_eq!(rounds.len(), 2);
}

#[test]
fn test_other_names_in_the_directory_are_not_segments() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 2, ONE_BLOCK);
    drop(wal);
    for name in [
        "x.segment",
        "123.segment",
        "0000000000000000000a.segment",
        "notes",
    ] {
        storage.put_file(&dir().join(name), b"junk").expect("put");
    }
    let (_, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.committed(), Position::new(2));
    assert_eq!(
        storage.contents(&dir().join("x.segment")),
        Some(b"junk".to_vec())
    );
}

#[test]
fn test_a_failed_directory_creation_is_an_io_error() {
    let storage = SimStorage::new(1);
    storage.fail_at(0, FaultMode::Before);
    let refusal = open(&storage).err().expect("refused");
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
    assert!(open(&storage).is_ok());
}

#[test]
fn test_a_tail_torn_in_its_header_is_named_by_its_position() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 3, ONE_BLOCK);
    drop(wal);
    // Round 3's header no longer verifies and nothing follows it: a torn tail, named as the round expected there.
    storage
        .corrupt(&segment(0), 8_192 + 50, 0x01)
        .expect("flipped");
    let (_, recovery) = open(&storage).expect("reopened");
    assert_eq!(
        recovery.truncation(),
        Some(Truncation::new(Position::new(3), 0, 8_192, Damage::Header))
    );
    assert_eq!(recovery.committed(), Position::new(2));
}

#[test]
fn test_preallocated_zeros_are_not_read_as_rounds() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    let checksums = append_synced(&mut wal, 1, 3, ONE_BLOCK);
    drop(wal);
    let (wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.committed(), Position::new(3));
    assert_eq!(recovery.truncation(), None);
    assert_eq!(wal.tail(), (Position::new(3), checksums[2]));
}

#[test]
fn test_a_torn_tail_is_truncated_reported_until_overwritten() {
    let storage = SimStorage::new(3);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 3, ONE_BLOCK);
    append(&mut wal, 4, FULL_BLOCK).expect("staged");
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    block_on(wal.sync()).expect_err("the torn write is reported");
    drop(wal);
    // The process restarts with the torn bytes in place.
    let (wal, recovery) = open(&storage).expect("reopened");
    let truncation = recovery.truncation().expect("a truncation");
    assert_eq!(truncation.position(), Position::new(4));
    assert_eq!((truncation.segment(), truncation.offset()), (0, 12_288));
    assert!(
        matches!(truncation.damage(), Damage::Chunk { position, index: 0, start: 0, end: 3_840 } if position == Position::new(4)),
        "{:?}",
        truncation.damage()
    );
    assert_eq!(recovery.committed(), Position::new(3));
    // The torn round's block is left for the next append to overwrite; until then each open reports it again.
    drop(wal);
    let (mut wal, again) = open(&storage).expect("reopened");
    assert_eq!(again.truncation(), recovery.truncation());
    append_synced(&mut wal, 4, 4, ONE_BLOCK);
    let bytes = storage.stable_contents(&segment(0)).expect("segment 0");
    assert_eq!(
        &bytes[12_288 + 256..12_288 + 1_256],
        body(4, ONE_BLOCK).as_slice()
    );
    assert!(
        bytes[12_288 + 1_256..].iter().all(|b| *b == 0),
        "nothing of the torn round is left"
    );
    append_synced(&mut wal, 5, 5, ONE_BLOCK);
    drop(wal);
    let (_, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.truncation(), None);
    assert_eq!(recovery.committed(), Position::new(5));
}

#[test]
fn test_a_torn_round_starting_a_segment_is_truncated_there() {
    let storage = SimStorage::new(3);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 4, ONE_BLOCK);
    append(&mut wal, 5, FULL_BLOCK).expect("staged");
    // Segment 1's creation, preallocation, sync and directory sync, then the round's write.
    storage.fail_at(storage.trace().len() + 4, FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(5)));
    drop(wal);
    storage.power_loss();
    let (_, recovery) = open(&storage).expect("reopened");
    let truncation = recovery.truncation().expect("a truncation");
    assert_eq!(truncation.position(), Position::new(5));
    assert_eq!((truncation.segment(), truncation.offset()), (1, 0));
    assert_eq!(recovery.committed(), Position::new(4));
}

#[test]
fn test_a_silent_torn_write_is_caught_and_named() {
    let storage = SimStorage::new(5);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 2, ONE_BLOCK);
    append(&mut wal, 3, FULL_TWO_BLOCKS).expect("staged");
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(3)), "acknowledged");
    drop(wal);
    storage.power_loss();
    let (mut wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(
        recovery.truncation().map(|t| t.position()),
        Some(Position::new(3))
    );
    assert_eq!(recovery.committed(), Position::new(2));
    assert_eq!(
        block_on(wal.read(Position::new(3), u32::MAX)),
        Ok(Vec::new())
    );
    assert_eq!(append(&mut wal, 3, ONE_BLOCK), Ok(Position::new(3)));
}

#[test]
fn test_a_truncation_is_a_warning_event() {
    let storage = SimStorage::new(3);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 2, ONE_BLOCK);
    append(&mut wal, 3, FULL_BLOCK).expect("staged");
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    block_on(wal.sync()).expect_err("torn");
    drop(wal);
    let events = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture(events.clone()));
    // Tracing caches a callsite's interest process-wide: a second dispatcher kept alive, and the cache rebuilt under
    // this one, keep a callsite another test thread reached first from being cached as uninteresting (as
    // ultragraphdb_store_sim's LogCapture does).
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        open(&storage).expect("reopened")
    });
    let events = events.lock().expect("events");
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
}

#[test]
fn test_interior_damage_refuses_the_start_naming_the_round() {
    let cases = [
        // A body byte of round 2: chunk 0, body bytes 0 to 1,000.
        (
            4_096 + 256 + 500,
            LogError::CorruptChunk(2, 0, 4_096, 0, 0, 1_000),
        ),
        // A header byte of round 2.
        (4_096 + 50, LogError::Corrupt(2, 0, 4_096)),
    ];
    for (offset, expected) in cases {
        let storage = SimStorage::new(1);
        let (mut wal, _) = open(&storage).expect("opened");
        append_synced(&mut wal, 1, 5, ONE_BLOCK);
        drop(wal);
        storage.corrupt(&segment(0), offset, 0x01).expect("flipped");
        let before = storage.trace().len();
        assert_eq!(open(&storage).err(), Some(expected));
        assert_eq!(writes_in(&storage, before), 0, "nothing is truncated");
    }
}

#[test]
fn test_a_chunk_damaged_in_a_later_segment_is_named_with_its_segment() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 3, ONE_BLOCK);
    append_synced(&mut wal, 4, 6, TWO_BLOCKS);
    drop(wal);
    // Round 4 starts segment 1, round 5 at its byte 8,192; chunk 1 of a 5,000-byte body is 3,840 to 5,000.
    storage
        .corrupt(&segment(1), 8_192 + 256 + 4_000, 0x01)
        .expect("flipped");
    assert_eq!(
        open(&storage).err(),
        Some(LogError::CorruptChunk(5, 1, 8_192, 1, 3_840, 5_000))
    );
}

#[test]
fn test_a_missing_segment_with_rounds_after_it_refuses_the_start() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 12, ONE_BLOCK);
    drop(wal);
    block_on(storage.remove_file(&segment(1))).expect("removed");
    assert_eq!(open(&storage).err(), Some(LogError::Corrupt(5, 1, 0)));
}

#[test]
fn test_junk_where_a_round_did_not_fit_is_passed_over() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 3, ONE_BLOCK);
    let checksums = append_synced(&mut wal, 4, 4, TWO_BLOCKS);
    drop(wal);
    // Round 4 did not fit segment 0's last block; a bit flips in that unused block.
    storage
        .corrupt(&segment(0), 12_288 + 7, 0x40)
        .expect("flipped");
    let (wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.truncation(), None);
    assert_eq!(wal.tail(), (Position::new(4), checksums[0]));
}

#[test]
fn test_the_checkpoint_must_match_the_rounds() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    let checksums = append_synced(&mut wal, 1, 6, ONE_BLOCK);
    drop(wal);
    storage
        .put_file(
            &superblock('b'),
            &Superblock::new(2, CLUSTER, 1, Position::new(5), 0xBAD, 0).encode(),
        )
        .expect("put");
    assert_eq!(
        open(&storage).err(),
        Some(LogError::Diverged(
            "the checkpoint's checksum",
            0xBAD,
            checksums[4]
        ))
    );
}

#[test]
fn test_a_torn_round_below_the_checkpoint_leaves_the_log_at_the_checkpoint() {
    let storage = SimStorage::new(9);
    let (mut wal, _) = open(&storage).expect("opened");
    let mut checksums = append_synced(&mut wal, 1, 4, ONE_BLOCK);
    append(&mut wal, 5, FULL_BLOCK).expect("staged");
    storage.fail_at(storage.trace().len() + 4, FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(5)));
    checksums.push(wal.tail().1);
    // The store synced through 5 and the checkpoint recorded it.
    block_on(wal.trim_through(Position::new(5))).expect("trimmed");
    drop(wal);
    storage.power_loss();
    let (mut wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(
        recovery.truncation().map(|t| t.position()),
        Some(Position::new(5))
    );
    assert_eq!(recovery.checkpoint(), Position::new(5));
    assert_eq!(recovery.committed(), Position::new(5));
    assert_eq!(wal.tail(), (Position::new(5), checksums[4]));
    append_synced(&mut wal, 6, 7, ONE_BLOCK);
    drop(wal);
    let (mut wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.truncation(), None);
    assert_eq!(recovery.committed(), Position::new(7));
    assert_eq!(
        block_on(wal.read(Position::new(6), u32::MAX))
            .expect("read")
            .len(),
        2
    );
}

#[test]
fn test_a_torn_round_at_the_checkpoint_after_rounds_below_it_leaves_the_log_at_the_checkpoint() {
    // Rounds 1 to 3 and the torn round 4 share segment 0; the checkpoint covers 4.
    let storage = SimStorage::new(9);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 3, ONE_BLOCK);
    append(&mut wal, 4, FULL_BLOCK).expect("staged");
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    assert_eq!(block_on(wal.sync()), Ok(Position::new(4)));
    let four = wal.tail().1;
    block_on(wal.trim_through(Position::new(4))).expect("trimmed");
    drop(wal);
    storage.power_loss();
    let (mut wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(
        recovery.truncation().map(|t| t.position()),
        Some(Position::new(4))
    );
    assert_eq!(recovery.committed(), Position::new(4));
    assert_eq!(wal.tail(), (Position::new(4), four));
    append_synced(&mut wal, 5, 6, ONE_BLOCK);
    drop(wal);
    let (_, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.truncation(), None);
    assert_eq!(recovery.committed(), Position::new(6));
}

#[test]
fn test_a_tail_segment_cut_short_is_preallocated_again() {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 2, ONE_BLOCK);
    drop(wal);
    let mut bytes = storage.contents(&segment(0)).expect("segment 0");
    bytes.truncate(8_192);
    storage.put_file(&segment(0), &bytes).expect("put");
    let (mut wal, recovery) = open(&storage).expect("reopened");
    assert_eq!(recovery.committed(), Position::new(2));
    assert_eq!(
        storage.stable_contents(&segment(0)).map(|b| b.len()),
        Some(16_384),
        "at the open, and synced"
    );
    append_synced(&mut wal, 3, 4, ONE_BLOCK);
    assert_eq!(storage.contents(&segment(0)).map(|b| b.len()), Some(16_384));
}

/// The level and fields of every event, the message under "message".
type Events = Arc<Mutex<Vec<(tracing::Level, BTreeMap<String, String>)>>>;

struct Capture(Events);

struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields(BTreeMap::new());
        event.record(&mut fields);
        self.0
            .lock()
            .expect("events")
            .push((*event.metadata().level(), fields.0));
    }
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
fn run_seed(seed: u64) {
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
                    return;
                }
            };
            let mut log = log;
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
            let rounds = block_on(log.read(Position::new(from), u32::MAX)).unwrap_or_default();
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
            let _ = log.next_event();
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
            _ => {
                let position = log.tail().0.get() + 1;
                let len = [10, ONE_BLOCK, TWO_BLOCKS][draws.next(3) as usize];
                match append(log, position, len) {
                    Ok(_) => {}
                    Err(LogError(LogErrorEnum::Busy { .. })) => {
                        let committed = log.committed();
                        if block_on(log.trim_through(committed)).is_err() {
                            wal = None;
                        }
                        storage.fail_randomly(0);
                        continue;
                    }
                    Err(other) => panic!("seed {seed}: {other:?}"),
                }
                match block_on(log.sync()) {
                    Ok(committed) => {
                        assert_eq!(committed.get(), position);
                        acknowledged.insert(position, log.tail().1);
                    }
                    Err(_) => {
                        wal = None;
                        if draws.next(2) == 0 {
                            storage.power_loss();
                        }
                    }
                }
            }
        }
        storage.fail_randomly(0);
    }
}

#[test]
fn test_no_acknowledged_round_is_lost_under_faults_and_power_losses() {
    for seed in 0..512 {
        run_seed(seed);
    }
}
