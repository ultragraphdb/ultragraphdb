/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The write-ahead-log scenarios, on the simulated storage. Where a scenario names the store, the store is a model
//! that applies what the log hands it at its committed position, and nothing else.

use std::collections::BTreeMap;

use cucumber::{World, then, when};
use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::{OrderedLog, Position, RoundRef};
use ultragraphdb_transactions_log::{
    Damage, FaultMode, Recovery, ScanStart, SimStorage, StorageOperation, Superblock, Wal,
    WalConfig, encode_round, scan_segment,
};

// The unit tests' helpers, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "helpers.rs"]
    pub mod helpers;
}

use crate::utils::helpers::{
    CLUSTER, DIR, FULL_TWO_BLOCKS, ONE_BLOCK, TWO_BLOCKS, append, append_synced, block_on, body,
    config, frame_checksum, open, segment, spare, superblock,
};

#[derive(Debug, Default, World)]
struct Steps {
    storage: Option<SimStorage>,
    refusals: Vec<LogError>,
    positions: Vec<u64>,
    recoveries: Vec<Recovery>,
    checks: Vec<bool>,
    trace_before: usize,
    trace_after: usize,
}

fn storage(w: &Steps) -> SimStorage {
    w.storage.clone().expect("a storage")
}

/// The rounds `wal` holds from `from`, by position, read and verified.
fn positions(wal: &mut Wal<SimStorage>, from: u64) -> Vec<u64> {
    block_on(wal.read(Position::new(from), u32::MAX))
        .expect("read")
        .iter()
        .map(|r| r.position().get())
        .collect()
}

// The ordered-log interface.

#[when("the log holds rounds 1 to 5 and round 7 is appended")]
fn append_seven(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 5, ONE_BLOCK);
    w.trace_before = storage.trace().len();
    let data = body(7, ONE_BLOCK);
    let parent = wal.tail().1;
    w.refusals = wal
        .append(RoundRef::new(Position::new(7), parent, 14, 1, &data))
        .err()
        .into_iter()
        .collect();
    w.positions = vec![block_on(wal.sync()).expect("nothing to sync").get()];
    w.trace_after = storage.trace().len();
    w.storage = Some(storage);
}

#[then("the append is refused with an error naming position 6, and nothing is written")]
fn refused_naming_six(w: &mut Steps) {
    assert_eq!(w.refusals, vec![LogError::PositionMismatch(6, 7)]);
    assert!(w.refusals[0].to_string().contains("expected 6"));
    assert_eq!(w.positions, vec![5]);
    assert_eq!(w.trace_after, w.trace_before);
}

#[when("the log is sealed at position 9 and a round is appended")]
fn seal_at_nine(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 9, ONE_BLOCK);
    w.positions = vec![wal.seal().expect("sealed").get()];
    w.refusals = append(&mut wal, 10, ONE_BLOCK).err().into_iter().collect();
}

#[then("the append fails, and `seal` reported 9")]
fn sealed_at_nine(w: &mut Steps) {
    assert_eq!(w.positions, vec![9]);
    assert_eq!(w.refusals, vec![LogError::Sealed(9)]);
}

// A round is durable before it is applied.

#[when("the node loses power at any step of a round")]
fn power_at_every_step(w: &mut Steps) {
    // Round 5 starts segment 1, so its steps are the segment's creation, preallocation, sync and directory sync, then
    // its write and its sync. The power is cut after each step in turn, the step failing before or after its effect;
    // the store applies a round only once the log reports it committed.
    for step in 0..6 {
        for mode in [FaultMode::Before, FaultMode::After] {
            let storage = SimStorage::new(step as u64);
            let (mut wal, _) = open(&storage).expect("opened");
            append_synced(&mut wal, 1, 4, ONE_BLOCK);
            let mut store: Vec<u64> = (1..=4).collect();
            append(&mut wal, 5, ONE_BLOCK).expect("staged");
            storage.fail_at(storage.trace().len() + step, mode);
            if let Ok(committed) = block_on(wal.sync()) {
                store.extend(5..=committed.get());
            }
            drop(wal);
            storage.power_loss();
            let (mut wal, _) = open(&storage).expect("recovered");
            let held = positions(&mut wal, 1);
            w.checks.push(store.iter().all(|p| held.contains(p)));
        }
    }
}

#[then("the store after recovery holds no write of a round the log does not hold")]
fn store_within_log(w: &mut Steps) {
    assert_eq!(w.checks.len(), 12);
    assert!(w.checks.iter().all(|held| *held));
}

// Rounds are frames in segments.

#[when("the log is opened with a segment holding three rounds followed by zeros")]
fn three_then_zeros(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 3, ONE_BLOCK);
    drop(wal);
    // A fresh segment is as long as its rounds; zeros follow them where a recovery cleared a torn tail.
    let mut bytes = storage.stable_contents(&segment(0)).expect("segment 0");
    bytes.resize(16_384, 0);
    storage.put_file(&segment(0), &bytes).expect("put");
    let (_, recovery) = open(&storage).expect("reopened");
    w.recoveries = vec![recovery];
    w.storage = Some(storage);
}

#[then("the scan finds three rounds and its tail after the third")]
fn three_rounds_found(w: &mut Steps) {
    assert_eq!(w.recoveries[0].committed(), Position::new(3));
    assert_eq!(w.recoveries[0].truncation(), None);
    let bytes = storage(w).contents(&segment(0)).expect("segment 0");
    let scan =
        scan_segment(&bytes, &ScanStart::new(CLUSTER, 1, Position::ZERO, 0)).expect("scanned");
    let found: Vec<u64> = scan.rounds().iter().map(|r| r.position().get()).collect();
    assert_eq!(found, [1, 2, 3]);
    // Three one-block rounds: the tail is the fourth block.
    assert_eq!(scan.end(), 12_288);
    assert_eq!(scan.damage(), None);
}

#[when("a reused segment holds rounds of a previous incarnation after the current rounds")]
fn reused_segment(w: &mut Steps) {
    // Incarnation 1 wrote rounds 1 to 8 over two segments; incarnation 2, created afresh, wrote its rounds 1 to 3 over
    // the start of segment 0. Its superblock names incarnation 2.
    let frames = |incarnation: u64, count: u64| -> Vec<u8> {
        let mut parent = 0;
        let mut bytes = Vec::new();
        for position in 1..=count {
            let data = body(position + incarnation * 100, ONE_BLOCK);
            let round = RoundRef::new(Position::new(position), parent, 2 * position, 1, &data);
            let (frame, checksum) = encode_round(CLUSTER, incarnation, &round).expect("encoded");
            bytes.extend_from_slice(frame.as_slice());
            parent = checksum;
        }
        bytes
    };
    let old = frames(1, 8);
    let mut first = old[..16_384].to_vec();
    let new = frames(2, 3);
    first[..new.len()].copy_from_slice(&new);
    let storage = SimStorage::new(1);
    storage.put_file(&segment(0), &first).expect("put");
    storage.put_file(&segment(1), &old[16_384..]).expect("put");
    storage
        .put_file(
            &superblock('a'),
            &Superblock::new(3, CLUSTER, 2, Position::ZERO, 0, 0).encode(),
        )
        .expect("put");
    let (mut wal, recovery) = open(&storage).expect("opened");
    w.recoveries = vec![recovery];
    w.positions = positions(&mut wal, 1);
    w.checks = vec![frame_checksum(&new[8_192..]) == wal.tail().1];
    w.storage = Some(storage);
}

#[then("the scan stops after the current incarnation's rounds")]
fn stopped_after_current(w: &mut Steps) {
    assert_eq!(w.recoveries[0].committed(), Position::new(3));
    assert_eq!(w.recoveries[0].truncation(), None);
    assert_eq!(w.positions, [1, 2, 3]);
    assert_eq!(w.checks, [true]);
}

// Recovery replays the log above the store.

#[when(
    "rounds 1 to 10 are synced in the log, the store had applied up to round 7, and power is lost"
)]
fn ten_synced_seven_applied(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 10, ONE_BLOCK);
    drop(wal);
    storage.power_loss();
    let mut store: Vec<u64> = (1..=7).collect();
    let (mut wal, _) = open(&storage).expect("recovered");
    store.extend(positions(&mut wal, 8));
    w.positions = store;
}

#[then("after recovery the store holds rounds 1 to 10")]
fn store_holds_ten(w: &mut Steps) {
    assert_eq!(w.positions, (1..=10).collect::<Vec<u64>>());
}

// Corruption is detected and never read as absence.

#[when(
    "the last round was torn by a crash during its write, before its sync completed, and no complete round follows it"
)]
fn torn_by_a_crash(w: &mut Steps) {
    // Rounds 1 and 2 take a block each; round 3 takes segment 0's last two, so its write is the next operation.
    let storage = SimStorage::new(4);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 2, ONE_BLOCK);
    append(&mut wal, 3, FULL_TWO_BLOCKS).expect("staged");
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    w.checks = vec![block_on(wal.sync()).is_err()];
    drop(wal);
    // The process dies with the torn bytes written; they are as the drive left them.
    let (mut wal, recovery) = open(&storage).expect("recovered");
    w.recoveries = vec![recovery];
    w.positions = positions(&mut wal, 1);
}

#[then(
    "the log is truncated before it, the truncation is reported, and the round was never acknowledged"
)]
fn truncated_before(w: &mut Steps) {
    let truncation = w.recoveries[0].truncation().expect("reported");
    assert_eq!(truncation.position(), Position::new(3));
    assert_eq!(w.recoveries[0].committed(), Position::new(2));
    assert_eq!(w.positions, [1, 2]);
    assert_eq!(
        w.checks,
        [true],
        "its sync failed, so it was never acknowledged"
    );
}

#[when("a byte of an acknowledged round's body is flipped and later rounds follow it")]
fn interior_flip(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 5, TWO_BLOCKS);
    drop(wal);
    // Round 3 is at byte 0 of segment 1; body byte 4,000 lies in chunk 1, body bytes 3,840 to 5,000.
    storage
        .corrupt(&segment(1), 256 + 4_000, 0x20)
        .expect("flipped");
    w.refusals = open(&storage).err().into_iter().collect();
}

#[then("the node refuses to start with an error naming the round, the chunk and its transactions")]
fn refused_naming_chunk(w: &mut Steps) {
    // The transactions whose records overlap a chunk are the round body's to name (task group 9); the log names the
    // chunk's byte range in the body so they can be.
    assert_eq!(
        w.refusals,
        vec![LogError::CorruptChunk(3, 1, 0, 1, 3_840, 5_000)]
    );
    let text = w.refusals[0].to_string();
    assert!(text.contains("round 3") && text.contains("chunk 1") && text.contains("3840..5000"));
}

// The superblock.

#[when("one superblock copy is corrupted")]
fn corrupt_copy(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 6, ONE_BLOCK);
    block_on(wal.trim_through(Position::new(2))).expect("both copies written");
    drop(wal);
    storage
        .corrupt(&superblock('a'), 30, 0x01)
        .expect("flipped");
    let segments = storage.contents(&segment(0));
    w.trace_before = storage.trace().len();
    w.refusals = open(&storage).err().into_iter().collect();
    let trace = storage.trace();
    w.checks = vec![
        trace[w.trace_before..].iter().all(|(op, _)| {
            !matches!(
                op,
                StorageOperation::Write | StorageOperation::Create | StorageOperation::Rename
            )
        }),
        storage.contents(&segment(0)) == segments,
    ];
}

#[then("the node refuses to start with an error naming the copy, and does not create a new log")]
fn refused_naming_copy(w: &mut Steps) {
    assert_eq!(w.refusals, vec![LogError::SuperblockUnreadable('a')]);
    assert!(w.refusals[0].to_string().contains("copy a"));
    assert_eq!(w.checks, [true, true]);
}

// Checkpoints trim the log.

#[when("rounds fill four segments and a checkpoint is taken at the end of the third")]
fn four_segments_checkpoint(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let (mut wal, _) = open(&storage).expect("opened");
    // Four one-block rounds fill a segment: rounds 1 to 16 fill segments 0 to 3, round 12 ends the third.
    append_synced(&mut wal, 1, 16, ONE_BLOCK);
    block_on(wal.trim_through(Position::new(12))).expect("trimmed");
    drop(wal);
    let (mut wal, recovery) = open(&storage).expect("recovered");
    w.recoveries = vec![recovery];
    w.positions = positions(&mut wal, 13);
    w.refusals = block_on(wal.read(Position::new(12), u32::MAX))
        .err()
        .into_iter()
        .collect();
    // Per segment index: the segment exists, and the spare exists holding only zeros.
    w.checks = (0..4)
        .flat_map(|i| {
            let zeroed_spare = storage
                .contents(&spare(i))
                .is_some_and(|bytes| !bytes.is_empty() && bytes.iter().all(|b| *b == 0));
            [storage.contents(&segment(i)).is_some(), zeroed_spare]
        })
        .collect();
}

#[then(
    "the first three segments are kept as spares holding only zeros, and recovery replays from the checkpoint"
)]
fn three_recycled(w: &mut Steps) {
    assert_eq!(
        w.checks,
        [false, true, false, true, false, true, true, false]
    );
    assert_eq!(w.recoveries[0].checkpoint(), Position::new(12));
    assert_eq!(w.recoveries[0].committed(), Position::new(16));
    assert_eq!(w.positions, [13, 14, 15, 16]);
    assert_eq!(w.refusals, vec![LogError::PositionMismatch(13, 12)]);
}

// The power-loss-protection flag.

#[when("the flag is `true` and a round is committed")]
fn flag_true_commit(w: &mut Steps) {
    let storage = SimStorage::new(1);
    let protected = WalConfig::new(CLUSTER, true)
        .with_segment_blocks(config().segment_blocks())
        .with_capacity(config().capacity());
    let (mut wal, _) = block_on(Wal::open(storage.clone(), DIR, protected)).expect("opened");
    append_synced(&mut wal, 1, 1, ONE_BLOCK);
    append(&mut wal, 2, ONE_BLOCK).expect("staged");
    let before_sync = wal.committed();
    w.trace_before = storage.trace().len();
    let committed = block_on(wal.sync()).expect("synced");
    w.positions = vec![before_sync.get(), committed.get()];
    w.checks = vec![
        storage.trace()[w.trace_before..]
            .iter()
            .map(|(op, _)| *op)
            .collect::<Vec<_>>()
            == [StorageOperation::Write, StorageOperation::FileSync],
    ];
    w.storage = Some(storage);
}

#[then("the round is synced before it is acknowledged")]
fn synced_before_acknowledged(w: &mut Steps) {
    // Not committed when appended; committed once its write and its sync completed.
    assert_eq!(w.positions, [1, 2]);
    assert_eq!(w.checks, [true]);
    let stable = storage(w).stable_contents(&segment(0)).expect("segment 0");
    assert_eq!(
        &stable[4_096 + 256..4_096 + 1_256],
        body(2, ONE_BLOCK).as_slice()
    );
}

// The log's storage is simulated.

#[when(
    "the simulated storage fails a round's sync with an error, or loses power after a round was acknowledged"
)]
fn reported_faults(w: &mut Steps) {
    for (seed, fault) in [(1, true), (2, false)] {
        let storage = SimStorage::new(seed);
        let (mut wal, _) = open(&storage).expect("opened");
        let mut acknowledged: BTreeMap<u64, u64> = append_synced(&mut wal, 1, 6, ONE_BLOCK)
            .into_iter()
            .zip(1..)
            .map(|(checksum, position)| (position, checksum))
            .collect();
        append(&mut wal, 7, ONE_BLOCK).expect("staged");
        if fault {
            // The write succeeds, its sync fails: never acknowledged.
            storage.fail_at(storage.trace().len() + 1, FaultMode::Before);
            assert!(block_on(wal.sync()).is_err());
        } else {
            block_on(wal.sync()).expect("acknowledged");
            acknowledged.insert(7, wal.tail().1);
        }
        drop(wal);
        storage.power_loss();
        let (mut wal, _) = open(&storage).expect("recovered");
        let rounds = block_on(wal.read(Position::new(1), u32::MAX)).expect("read");
        let present: BTreeMap<u64, u64> = rounds
            .iter()
            .map(|r| (r.position().get(), r.checksum()))
            .collect();
        w.checks
            .push(acknowledged.iter().all(|(p, c)| present.get(p) == Some(c)));
    }
}

#[then("after recovery every acknowledged round is present")]
fn acknowledged_present(w: &mut Steps) {
    assert_eq!(w.checks, [true, true]);
}

#[when(
    "the simulated storage tears the last round's write without reporting it, and the node restarts"
)]
fn silent_tear(w: &mut Steps) {
    let storage = SimStorage::new(6);
    let (mut wal, _) = open(&storage).expect("opened");
    append_synced(&mut wal, 1, 2, ONE_BLOCK);
    append(&mut wal, 3, FULL_TWO_BLOCKS).expect("staged");
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    w.positions = vec![block_on(wal.sync()).expect("reported as synced").get()];
    drop(wal);
    storage.power_loss();
    let (mut wal, recovery) = open(&storage).expect("recovered");
    w.recoveries = vec![recovery];
    w.positions.extend(positions(&mut wal, 1));
    w.storage = Some(storage);
}

#[then(
    "recovery detects the round by its checksums, truncates it as a torn tail and reports the truncation; it never reads the round as valid"
)]
fn silent_tear_caught(w: &mut Steps) {
    // The sync reported 3: the round was acknowledged, then lost with the tear, and named.
    assert_eq!(w.positions[0], 3);
    let truncation = w.recoveries[0].truncation().expect("reported");
    assert_eq!(truncation.position(), Position::new(3));
    assert!(matches!(
        truncation.damage(),
        Damage::Chunk { .. } | Damage::ChunkTable { .. }
    ));
    assert_eq!(w.recoveries[0].committed(), Position::new(2));
    assert_eq!(&w.positions[1..], [1, 2]);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
