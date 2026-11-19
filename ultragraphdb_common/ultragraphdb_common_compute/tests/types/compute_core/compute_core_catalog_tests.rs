/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A catalog version read from the system group across its Catalog frames (`PROTOCOL.md` §11.3, §13.7; design D13), at the
//! group's address compute dials (corner case C6.4).

use std::rc::Rc;
use std::time::Duration;

use ultragraphdb_common_compute::{ComputeCore, ReadTimes};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{CatalogFrames, ErrorCode, ErrorReply, MAX_FRAME_SIZE};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played::played_cluster::{SECRET, quiet_times, role_cluster, run};
use crate::played::played_system::{PlayedSystem, even_ranges};

/// The tenant the versions belong to.
const TENANT: u64 = 4;

/// A played group and a core reading from it by `times`.
async fn group_and_core(times: ReadTimes) -> TestResult<(PlayedSystem, Rc<ComputeCore>)> {
    let cluster = role_cluster(SECRET, Duration::from_secs(5))?;
    let group = PlayedSystem::start(&cluster, even_ranges(1)?).await?;
    let core = ComputeCore::new(cluster, Address::parse(group.address())?, times, "compute");
    Ok((group, core))
}

/// `length` bytes of a pattern no frame boundary lines up with: byte `i` is `i` modulo 251, a prime.
fn pattern(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| u8::try_from(index % 251).unwrap_or_default())
        .collect()
}

/// The version `frames` read: its tenant, number, epoch and bytes, once the frame flagged last was taken.
fn read_back(frames: &CatalogFrames) -> TestResult<(u64, u64, u64, Vec<u8>)> {
    assert!(frames.is_last(), "the version is whole");
    let view = frames.view().context("a version read")?;
    Ok((
        view.tenant(),
        view.version(),
        view.epoch(),
        view.bytes().to_vec(),
    ))
}

/// A version of two and a half frames' bytes at compute's `max_frame` is read whole, its bytes those the group holds in
/// their order, its number and epoch the reply's, and the group is asked once.
#[test]
fn test_a_version_past_one_frame_is_read_whole_across_frames() -> TestResult {
    run(async {
        let (group, core) = group_and_core(quiet_times()).await?;
        // Two and a half of compute's largest frames: three frames, the last part full.
        let bytes = pattern(MAX_FRAME_SIZE * 5 / 2);
        group.hold_version(TENANT, 3, 9, bytes.clone());
        let frames = core.catalog(TENANT, 3).await.map_err(refused)?;
        assert_eq!(read_back(&frames)?, (TENANT, 3, 9, bytes));
        assert_eq!(group.catalog_asked(), 1);
        Ok(())
    })
}

/// A version of one frame and an empty version are each read as held.
#[test]
fn test_a_version_of_one_frame_and_an_empty_one_are_read() -> TestResult {
    run(async {
        let (group, core) = group_and_core(quiet_times()).await?;
        group.hold_version(TENANT, 1, 2, Vec::new());
        group.hold_version(TENANT, 2, 5, pattern(1_000));
        let empty = core.catalog(TENANT, 1).await.map_err(refused)?;
        assert_eq!(read_back(&empty)?, (TENANT, 1, 2, Vec::new()));
        let small = core.catalog(TENANT, 2).await.map_err(refused)?;
        assert_eq!(read_back(&small)?, (TENANT, 2, 5, pattern(1_000)));
        Ok(())
    })
}

/// A request of `version` 0 reads the tenant's current version, the group's answer to it.
#[test]
fn test_version_0_reads_the_current_version() -> TestResult {
    run(async {
        let (group, core) = group_and_core(quiet_times()).await?;
        group.hold_version(TENANT, 1, 2, pattern(10));
        group.hold_version(TENANT, 2, 7, pattern(20));
        let current = core.catalog(TENANT, 0).await.map_err(refused)?;
        assert_eq!(read_back(&current)?, (TENANT, 2, 7, pattern(20)));
        Ok(())
    })
}

/// A version the tenant never held is refused with the group's own Error: code 7, `detail` the version (`PROTOCOL.md`
/// §13.7).
#[test]
fn test_a_version_never_held_is_refused_with_the_groups_error() -> TestResult {
    run(async {
        let (group, core) = group_and_core(quiet_times()).await?;
        group.hold_version(TENANT, 1, 2, pattern(10));
        let refusal = core.catalog(TENANT, 9).await.err().context("a refusal")?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        assert_eq!(refusal.detail(), 9);
        Ok(())
    })
}

/// A group that answers past the answer bound, or turns the connection away, refuses the read of a version with code 17,
/// naming the catalog.
#[test]
fn test_a_group_that_does_not_answer_refuses_with_code_17() -> TestResult {
    run(async {
        // The answer bound is 1 s: twice the 500 ms stall.
        let times = ReadTimes::new(Duration::from_millis(500), Duration::from_secs(60));
        let (group, core) = group_and_core(times).await?;
        group.hold_version(TENANT, 1, 2, pattern(10));
        group.delay_catalog(Duration::from_secs(3));
        let slow = core.catalog(TENANT, 1).await.err().context("a refusal")?;
        assert_eq!(slow.code(), ErrorCode::Unavailable);
        assert!(slow.message().contains("catalog"), "{}", slow.message());
        group.delay_catalog(Duration::ZERO);
        group.answer(false);
        let gone = core.catalog(TENANT, 1).await.err().context("a refusal")?;
        assert_eq!(gone.code(), ErrorCode::Unavailable);
        assert!(gone.message().contains("catalog"), "{}", gone.message());
        Ok(())
    })
}

/// The most Catalog frames compute reads for one version: `catalog_bytes_max`, 16,777,216 bytes, over half the `max_frame`
/// floor, 262,144 bytes, is 64 frames filled at the least, and one more for the last (`notes/group-6.md`, *Found wanting* 2).
const FRAMES_MAX: usize = 65;

/// A version answered in the most frames compute takes, one byte to a frame, is read whole; one frame more is refused with
/// code 17 naming the catalog, past which compute reads no further frame: the connection compute dialled closes on a frame
/// it refused (`PROTOCOL.md` §11.12, *Errors*).
#[test]
fn test_a_version_in_the_most_frames_is_read_and_one_more_refused() -> TestResult {
    run(async {
        let (group, core) = group_and_core(quiet_times()).await?;
        group.hold_version(TENANT, 1, 2, pattern(FRAMES_MAX));
        group.hold_version(TENANT, 2, 3, pattern(FRAMES_MAX + 1));
        group.answer_in(FRAMES_MAX);
        let most = core.catalog(TENANT, 1).await.map_err(refused)?;
        assert_eq!(read_back(&most)?, (TENANT, 1, 2, pattern(FRAMES_MAX)));
        group.answer_in(FRAMES_MAX + 1);
        let past = core.catalog(TENANT, 2).await.err().context("a refusal")?;
        assert_eq!(past.code(), ErrorCode::Unavailable);
        assert!(past.message().contains("catalog"), "{}", past.message());
        Ok(())
    })
}

/// A frame breaking the first's command header, a version's second frame naming another version, is refused with code 17
/// naming the catalog, as is every frame compute refuses on the connection it dialled (`PROTOCOL.md` §11.12, *Errors*;
/// §13.7); the same version answered whole is read.
#[test]
fn test_a_frame_breaking_the_first_ones_header_is_refused_with_code_17() -> TestResult {
    run(async {
        let (group, core) = group_and_core(quiet_times()).await?;
        group.hold_version(TENANT, 1, 2, pattern(3));
        group.answer_in(3);
        group.answer_torn(true);
        let torn = core.catalog(TENANT, 1).await.err().context("a refusal")?;
        assert_eq!(torn.code(), ErrorCode::Unavailable);
        assert!(torn.message().contains("catalog"), "{}", torn.message());
        group.answer_torn(false);
        let whole = core.catalog(TENANT, 1).await.map_err(refused)?;
        assert_eq!(read_back(&whole)?, (TENANT, 1, 2, pattern(3)));
        Ok(())
    })
}

/// A refusal as a test's error.
fn refused(error: ErrorReply) -> ultragraphdb_tool_bdd::TestError {
    ultragraphdb_tool_bdd::TestError::new(format!("refused: {error:?}"))
}
