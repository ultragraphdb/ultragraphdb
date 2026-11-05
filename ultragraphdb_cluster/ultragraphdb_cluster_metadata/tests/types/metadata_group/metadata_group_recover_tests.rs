/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_cluster_metadata::{
    MetadataCommand, MetadataConfig, MetadataSnapshot, MetadataState,
};
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_log::LogError;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_traits_log::{DurabilityTicket, LogEvent, OrderedLog, Position, Round, RoundRef};
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::harness::{
    Log, block_on, crash_and_recover, fresh, open, pid, recover, sync, tenant,
};

/// Writes `bodies` as the rounds of a fresh log in `storage`, synced.
fn write_log(storage: &SimStorage, bodies: &[Vec<u8>]) -> TestResult {
    let mut log = open(storage)?;
    for body in bodies {
        let (tail, parent) = log.tail();
        let position = tail.next().context("a next position")?;
        let _ticket = log.append(RoundRef::new(position, parent, 0, 1, NonZeroU32::MIN, body))?;
    }
    let _durable = block_on(log.sync())??;
    Ok(())
}

/// The command's encoding.
fn encoded(command: &MetadataCommand) -> Vec<u8> {
    let mut out = Vec::new();
    command.encode(&mut out);
    out
}

#[test]
fn test_recovery_replays_every_durable_command_in_order() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 0, b"v1")?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 1, b"v2")?;
    let _: (SequenceBlock, Position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 9)?;
    sync(&mut group)?;
    let before = group.state().clone();
    let recovered = crash_and_recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(recovered.state(), &before);
    assert_eq!(recovered.applied().get(), 3);
    assert_eq!(recovered.replayed(), 3);
    Ok(())
}

#[test]
fn test_a_command_not_durable_at_the_kill_is_lost_whole() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 0, b"v1")?;
    sync(&mut group)?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 1, b"v2")?;
    let recovered = crash_and_recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(recovered.state().current_version(tenant()), 1);
    assert_eq!(recovered.applied().get(), 1);
    Ok(())
}

#[test]
fn test_a_recovered_group_goes_on_deciding_from_its_state() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _: (SequenceBlock, Position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 9)?;
    let _: (u64, Position) = group.create_principal(
        "ann",
        crate::utils::harness::verifier("ann's password!", 1)?,
        true,
    )?;
    sync(&mut group)?;
    let mut recovered = crash_and_recover(&storage, MetadataConfig::new(), None)?;
    let (block, position) = recovered.lease_block(TypePrefix::RESOURCE, pid(2)?, 1)?;
    assert_eq!((block.start().get(), position.get()), (10, 3));
    let (id, _) = recovered.create_principal(
        "bob",
        crate::utils::harness::verifier("bob's password!", 2)?,
        false,
    )?;
    assert_eq!(id, 2);
    Ok(())
}

#[test]
fn test_recovery_from_a_snapshot_replays_only_the_rounds_after_it() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 0, b"v1")?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 1, b"v2")?;
    sync(&mut group)?;
    let snapshot = group.snapshot();
    let _: (u64, Position) = group.offer_catalog(tenant(), 2, b"v3")?;
    sync(&mut group)?;
    let before = group.state().clone();
    let recovered = crash_and_recover(&storage, MetadataConfig::new(), Some(snapshot))?;
    assert_eq!(recovered.replayed(), 1);
    assert_eq!(recovered.state(), &before);
    assert_eq!(recovered.applied().get(), 3);
    Ok(())
}

#[test]
fn test_a_snapshot_at_the_committed_position_replays_nothing() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 0, b"v1")?;
    sync(&mut group)?;
    let snapshot = group.snapshot();
    let recovered = crash_and_recover(&storage, MetadataConfig::new(), Some(snapshot))?;
    assert_eq!(recovered.replayed(), 0);
    assert_eq!(recovered.applied().get(), 1);
    assert_eq!(recovered.state().current_version(tenant()), 1);
    Ok(())
}

#[test]
fn test_a_snapshot_past_the_committed_position_is_refused() -> TestResult {
    let storage = SimStorage::new(3);
    let snapshot = MetadataSnapshot::new(Position::new(2), MetadataState::new());
    let refusal = block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
        open(&storage)?,
        MetadataConfig::new(),
        Some(snapshot),
    ))?
    .err()
    .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterError::replay_diverged(2, "a snapshot at or below the log's committed position").0
    );
    Ok(())
}

#[test]
fn test_a_trimmed_log_needs_a_snapshot_at_or_past_its_checkpoint() -> TestResult {
    let (storage, mut group) = fresh()?;
    for predecessor in 0..3 {
        let _: (u64, Position) = group.offer_catalog(tenant(), predecessor, b"v")?;
    }
    sync(&mut group)?;
    let old = group.snapshot();
    let _: (u64, Position) = group.offer_catalog(tenant(), 3, b"v")?;
    sync(&mut group)?;
    let snapshot = group.snapshot();
    block_on(group.trim(&snapshot))??;
    drop(group);
    let refused = |offered| -> TestResult<ClusterErrorEnum> {
        Ok(
            block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
                open(&storage)?,
                MetadataConfig::new(),
                offered,
            ))?
            .err()
            .context("a refusal")?
            .0,
        )
    };
    assert_eq!(refused(None)?, ClusterError::snapshot_behind(0, 4).0);
    assert_eq!(refused(Some(old))?, ClusterError::snapshot_behind(3, 4).0);
    let recovered = recover(&storage, MetadataConfig::new(), Some(snapshot))?;
    assert_eq!(recovered.state().current_version(tenant()), 4);
    Ok(())
}

#[test]
fn test_a_round_that_is_no_command_is_refused() -> TestResult {
    let storage = SimStorage::new(5);
    write_log(&storage, &[vec![99]])?;
    let refusal = block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
        open(&storage)?,
        MetadataConfig::new(),
        None,
    ))?
    .err()
    .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterError::malformed("a kind byte, 1 to 13", 0).0
    );
    Ok(())
}

#[test]
fn test_a_command_the_state_does_not_take_diverges_at_its_position() -> TestResult {
    let storage = SimStorage::new(6);
    // Version 1 offered twice: the second is built on 0 while 1 is the latest.
    let offer = encoded(&MetadataCommand::OfferCatalog {
        tenant: tenant(),
        predecessor: 0,
        bytes: b"v".to_vec(),
    });
    write_log(&storage, &[offer.clone(), offer])?;
    let refusal = block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
        open(&storage)?,
        MetadataConfig::new(),
        None,
    ))?
    .err()
    .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterError::replay_diverged(2, "a command the state before it takes").0
    );
    Ok(())
}

#[test]
fn test_a_close_announcing_a_placement_version_not_pending_diverges_at_its_position() -> TestResult
{
    let storage = SimStorage::new(16);
    let init = encoded(&MetadataCommand::InitPlacement {
        partitions: 2,
        leader: ultragraphdb_cluster_placement::NodeId::new(1),
    });
    let change = encoded(&MetadataCommand::ChangePlacement {
        assignments: vec![(
            ultragraphdb_cluster_placement::BucketRange::new(0, 16_777_216)?,
            pid(1)?,
        )],
    });
    // Placement version 1 is pending; a close announcing version 2 is no close the group wrote.
    let close = encoded(&MetadataCommand::LearnClose {
        epoch: ultragraphdb_core_ids::Epoch::new(5)?,
        versions: Vec::new(),
        placement: 2,
        evictions: Vec::new(),
    });
    write_log(&storage, &[init, change, close])?;
    let refusal = block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
        open(&storage)?,
        MetadataConfig::new(),
        None,
    ))?
    .err()
    .context("a refusal")?;
    assert_eq!(
        refusal,
        ClusterError::replay_diverged(3, "a close announces the oldest pending placement version")
    );
    Ok(())
}

#[test]
fn test_recovery_reads_a_log_longer_than_one_read() -> TestResult {
    // 12 rounds of 100 KiB each pass the 1 MiB one read returns; the test log's segments hold 256 KiB each.
    let (storage, mut group) = fresh()?;
    for predecessor in 0..12 {
        let _: (u64, Position) =
            group.offer_catalog(tenant(), predecessor, &vec![7; 100 * 1_024])?;
        sync(&mut group)?;
    }
    let recovered = crash_and_recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(recovered.replayed(), 12);
    assert_eq!(recovered.state().current_version(tenant()), 12);
    Ok(())
}

/// The chunk at `offset` holding `bytes`.
fn chunk(offset: u64, bytes: &[u8]) -> Vec<u8> {
    encoded(&MetadataCommand::Chunk {
        offset,
        bytes: bytes.to_vec(),
    })
}

/// The commit of `whole`, its length and its XXH3-64.
fn commit_of(whole: &[u8]) -> Vec<u8> {
    encoded(&MetadataCommand::Commit {
        length: whole.len() as u64,
        checksum: xxhash_rust::xxh3::xxh3_64(whole),
    })
}

/// The refusal of recovering from a log holding `bodies`.
fn replay_refusal(bodies: &[Vec<u8>]) -> TestResult<ClusterErrorEnum> {
    let storage = SimStorage::new(8);
    write_log(&storage, bodies)?;
    Ok(
        block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
            open(&storage)?,
            MetadataConfig::new(),
            None,
        ))?
        .err()
        .context("a refusal")?
        .0,
    )
}

/// The offer of version 1 of the scenarios' tenant holding `bytes`.
fn offer_1(bytes: &[u8]) -> Vec<u8> {
    encoded(&MetadataCommand::OfferCatalog {
        tenant: tenant(),
        predecessor: 0,
        bytes: bytes.to_vec(),
    })
}

#[test]
fn test_chunks_with_their_commit_replay_as_one_command() -> TestResult {
    // Chunks are written by hand here, so any split of a command replays: three pieces of uneven length.
    let whole = offer_1(b"a version in three pieces");
    let (a, rest) = whole.split_at(5);
    let (b, c) = rest.split_at(11);
    let storage = SimStorage::new(9);
    write_log(
        &storage,
        &[chunk(0, a), chunk(5, b), chunk(16, c), commit_of(&whole)],
    )?;
    let group = recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(group.replayed(), 1);
    assert_eq!(group.applied().get(), 4);
    let held = group.state().versions(tenant());
    assert_eq!(
        held.first().map(|v| v.bytes()),
        Some(&b"a version in three pieces"[..])
    );
    Ok(())
}

#[test]
fn test_chunks_with_no_commit_are_discarded_and_a_retry_commits() -> TestResult {
    let whole = offer_1(b"cut short");
    let (a, b) = whole.split_at(9);
    let storage = SimStorage::new(10);
    // At the log's end: a crash cut the run.
    write_log(&storage, &[chunk(0, a), chunk(9, b)])?;
    let mut group = recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(group.state().current_version(tenant()), 0);
    assert_eq!((group.applied().get(), group.replayed()), (0, 0));
    // The retry is appended after the cut run, which a new run's first chunk or any command discards on replay.
    let (version, position) = group.offer_catalog(tenant(), 0, b"retried")?;
    assert_eq!((version, position.get()), (1, 3));
    sync(&mut group)?;
    let recovered = crash_and_recover(&storage, MetadataConfig::new(), None)?;
    let held_retried = recovered.state().versions(tenant());
    assert_eq!(
        held_retried.first().map(|v| v.bytes()),
        Some(&b"retried"[..])
    );
    assert_eq!(held_retried.len(), 1);
    // A run cut short and followed by a new run from offset 0.
    let other = SimStorage::new(12);
    let retry = offer_1(b"second run");
    let (c, d) = retry.split_at(4);
    write_log(
        &other,
        &[chunk(0, a), chunk(0, c), chunk(4, d), commit_of(&retry)],
    )?;
    let again = recover(&other, MetadataConfig::new(), None)?;
    let held_again = again.state().versions(tenant());
    assert_eq!(
        held_again.first().map(|v| v.bytes()),
        Some(&b"second run"[..])
    );
    Ok(())
}

#[test]
fn test_a_crash_between_chunks_leaves_the_version_absent() -> TestResult {
    // A version of 300 KiB in three chunks and a commit, the run's sync failed at each storage operation in turn: the
    // recovered group holds the version whole or not at all, and a retry commits it.
    let large = vec![4; 300 * 1_024];
    let (clean, mut probe) = crate::utils::harness::fresh()?;
    let start = clean.trace().len();
    let _: (u64, Position) = probe.offer_catalog(tenant(), 0, &large)?;
    sync(&mut probe)?;
    let end = clean.trace().len();
    let mut cut = 0;
    for operation in start..end {
        let (storage, mut group) = crate::utils::harness::fresh()?;
        let _: (u64, Position) = group.offer_catalog(tenant(), 0, &large)?;
        storage.fail_at(operation, ultragraphdb_transactions_log::FaultMode::Before);
        let _synced = block_on(group.sync())?;
        drop(group);
        let mut recovered = crash_and_recover(&storage, MetadataConfig::new(), None)?;
        let held = recovered.state().versions(tenant());
        match held.first() {
            Some(record) => assert_eq!(record.bytes(), large.as_slice(), "operation {operation}"),
            None => {
                let committed = open_committed(&storage)?;
                cut += usize::from(committed > 0);
                // The snapshot holds no half-written command: it is the state before the run.
                assert_eq!(recovered.snapshot().position(), Position::ZERO);
                assert_eq!(recovered.snapshot().state(), &MetadataState::new());
                let (version, _) = recovered.offer_catalog(tenant(), 0, &large)?;
                assert_eq!(version, 1);
                sync(&mut recovered)?;
                assert_eq!(recovered.state().current_version(tenant()), 1);
            }
        }
    }
    assert!(cut > 0, "no fault left chunks without their commit");
    Ok(())
}

/// The committed position of the log of `storage`, reopened.
fn open_committed(storage: &SimStorage) -> TestResult<u64> {
    Ok(open(storage)?.committed().get())
}

#[test]
fn test_chunks_out_of_order_or_a_commit_not_naming_them_diverge() -> TestResult {
    let whole = offer_1(b"framed");
    let (a, b) = whole.split_at(6);
    let diverged = |position, what| ClusterError::replay_diverged(position, what).0;
    assert_eq!(
        replay_refusal(&[chunk(0, a), chunk(7, b)])?,
        diverged(2, "a chunk at the end of the chunks before it")
    );
    assert_eq!(
        replay_refusal(&[chunk(3, a)])?,
        diverged(1, "a chunk at the end of the chunks before it")
    );
    assert_eq!(
        replay_refusal(&[commit_of(&whole)])?,
        diverged(1, "a commit naming the chunks before it")
    );
    let mut wrong_sum = commit_of(&whole);
    if let Some(byte) = wrong_sum.last_mut() {
        *byte ^= 1;
    }
    assert_eq!(
        replay_refusal(&[chunk(0, a), chunk(6, b), wrong_sum])?,
        diverged(3, "a commit naming the chunks before it")
    );
    let short = encoded(&MetadataCommand::Commit {
        length: whole.len() as u64 - 1,
        checksum: xxhash_rust::xxh3::xxh3_64(&whole),
    });
    assert_eq!(
        replay_refusal(&[chunk(0, a), chunk(6, b), short])?,
        diverged(3, "a commit naming the chunks before it")
    );
    // Chunks holding bytes that are no command, or another chunk.
    let junk = [99_u8, 1, 2];
    assert_eq!(
        replay_refusal(&[chunk(0, &junk), commit_of(&junk)])?,
        diverged(2, "chunks holding a command")
    );
    let nested = chunk(0, b"x");
    assert_eq!(
        replay_refusal(&[chunk(0, &nested), commit_of(&nested)])?,
        diverged(2, "a whole command, not a chunk of one or its commit")
    );
    Ok(())
}

/// A log of segments of 4 MiB, 16 of them, in `storage`: room for the chunks of the largest command.
fn large_log(storage: &SimStorage) -> TestResult<Log> {
    let config = crate::utils::harness::wal_config()
        .with_segment_blocks(NonZeroU32::MIN.saturating_add(1_023))
        .with_capacity(NonZeroU32::MIN.saturating_add(15));
    let (log, _) = block_on(ultragraphdb_transactions_log::Wal::open(
        storage.clone(),
        crate::utils::harness::DIR,
        config,
    ))??;
    Ok(log)
}

/// Appends `bodies` to the large log of `storage` as its next rounds, synced.
fn append_large(storage: &SimStorage, bodies: impl Iterator<Item = Vec<u8>>) -> TestResult {
    let mut log = large_log(storage)?;
    for body in bodies {
        let (tail, parent) = log.tail();
        let position = tail.next().context("a next position")?;
        let _ticket = log.append(RoundRef::new(
            position,
            parent,
            0,
            1,
            NonZeroU32::MIN,
            &body,
        ))?;
    }
    let _durable = block_on(log.sync())??;
    Ok(())
}

/// The group recovered from the large log of `storage`, with no snapshot.
fn recover_large(
    storage: &SimStorage,
) -> TestResult<Result<crate::utils::harness::Group, ClusterError>> {
    let log = large_log(storage)?;
    block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
        log,
        MetadataConfig::new(),
        None,
    ))
}

#[test]
fn test_chunks_of_the_most_bytes_of_a_command_are_taken() -> TestResult {
    // 256 chunks of 128 KiB, 32 MiB, the most a command holds, with no commit: taken, then discarded at the log's end.
    let storage = SimStorage::new(14);
    let piece = vec![6; MetadataCommand::CHUNK_BYTES];
    let chunks =
        (0..256_u64).map(|index| chunk(index * MetadataCommand::CHUNK_BYTES as u64, &piece));
    append_large(&storage, chunks)?;
    let group = recover_large(&storage)??;
    assert_eq!((group.applied().get(), group.replayed()), (0, 0));
    drop(group);
    // One chunk more passes the most bytes of a command: the run diverges at that chunk.
    let one_more = chunk(256 * MetadataCommand::CHUNK_BYTES as u64, &[6]);
    append_large(&storage, std::iter::once(one_more))?;
    let refusal = recover_large(&storage)?.err().context("a refusal")?;
    assert_eq!(
        refusal,
        ClusterError::replay_diverged(257, "chunks within the most bytes of a command")
    );
    Ok(())
}

/// A log reading its rounds back one position further than they were written: a log breaking its own order.
struct ShiftedLog(Log);

impl OrderedLog for ShiftedLog {
    fn append(&mut self, round: RoundRef<'_>) -> Result<DurabilityTicket, LogError> {
        self.0.append(round)
    }

    fn poll_ticket(&self, ticket: DurabilityTicket) -> std::task::Poll<Result<(), LogError>> {
        self.0.poll_ticket(ticket)
    }

    fn sync(&mut self) -> impl Future<Output = Result<Position, LogError>> {
        self.0.sync()
    }

    fn committed(&self) -> Position {
        self.0.committed()
    }

    fn tail(&self) -> (Position, u64) {
        self.0.tail()
    }

    async fn read(&mut self, from: Position, max_bytes: u32) -> Result<Vec<Round>, LogError> {
        let rounds = self.0.read(from, max_bytes).await?;
        Ok(rounds
            .into_iter()
            .map(|r| {
                Round::new(
                    Position::new(r.position().get() + 1),
                    r.parent(),
                    r.checksum(),
                    r.first_version(),
                    r.transactions(),
                    r.derivation(),
                    r.body().to_vec(),
                )
            })
            .collect())
    }

    fn trim_through(&mut self, position: Position) -> impl Future<Output = Result<(), LogError>> {
        self.0.trim_through(position)
    }

    fn checkpoint(&self) -> Position {
        self.0.checkpoint()
    }

    fn seal(&mut self) -> Result<Position, LogError> {
        self.0.seal()
    }

    fn next_event(&mut self) -> Option<LogEvent> {
        self.0.next_event()
    }
}

#[test]
fn test_a_round_read_out_of_its_position_diverges() -> TestResult {
    let storage = SimStorage::new(15);
    write_log(&storage, &[offer_1(b"v1")])?;
    let refusal = block_on(ultragraphdb_cluster_metadata::MetadataGroup::recover(
        ShiftedLog(open(&storage)?),
        MetadataConfig::new(),
        None,
    ))?
    .err()
    .context("a refusal")?;
    assert_eq!(
        refusal,
        ClusterError::replay_diverged(2, "a round at the position after the one before it")
    );
    Ok(())
}
