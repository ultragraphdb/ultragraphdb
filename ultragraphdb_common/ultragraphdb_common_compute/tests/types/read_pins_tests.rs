/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What keeps the versions a read chose readable (`PROTOCOL.md` §11.9, quad-reads "A partition retains what a read may still
//! read"): a Scan holding each partition's version, from the frame that first says it for a version chosen before the read began,
//! taking a frame per `scan_stall_ms / 3`, the next opened while the read goes on taking frames, and none for a short read whose
//! version it chose as it began.

use std::time::Duration;

use ultragraphdb_common_compute::{ReadTimes, read_times_of};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_wire::{ComputeRead, Credit, MAX_FRAME_FLOOR, ReadLevel, ScanAt};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::played_answers::{Batch, Pattern, answer_on, next_of, read_of, snapshot};
use crate::utils::played_cluster::{Partition, Played, id, run, version, within};

const P: &str = "http://ex.org/p";

/// The times of a cluster whose `scan_stall_ms` is 150: a held version's Scan takes a frame each 50 ms, and its 16 ranges last
/// about 750 ms.
fn quick() -> ReadTimes {
    read_times_of(Some(150), None)
}

/// How long one held Scan lasts at [`quick`]'s times: its 15 grants after the first frame, 50 ms apart.
const ONE_SCAN: Duration = Duration::from_millis(750);

/// `partition` holding `rows` rows `<http://ex.org/s{i}> <p> <o{i}>`, each object's IRI of 300,000 bytes, so each row is a Result
/// frame of its own; IDs from `base`.
fn hold_rows(partition: &Partition, rows: u64, base: u64) -> TestResult {
    let predicate = id(1)?;
    partition.term(P, predicate);
    for row in 0..rows {
        let (s, o) = (id(base + row * 2)?, id(base + row * 2 + 1)?);
        partition.term(&format!("http://ex.org/s{base}/{row}"), s);
        partition.term(
            &format!("http://ex.org/o{base}/{row}/{}", "a".repeat(300_000)),
            o,
        );
        partition.quads(
            ultragraphdb_core_keys::Graph::Default,
            s,
            predicate,
            &[ultragraphdb_core_terms::EncodedTerm::id(o)],
        );
    }
    Ok(())
}

/// `read` sent on `client` with a credit of one frame: its first batch.
async fn first_batch(client: &mut RoleClient, read: ComputeRead) -> TestResult<(u64, Batch)> {
    client
        .send_only(&read.with_client_limits(MAX_FRAME_FLOOR as u32, 1))
        .await?;
    let stream = client.last_stream();
    let batch = next_of(client).await?.map_err(|refused| {
        ultragraphdb_tool_bdd::TestError::new(format!("refused: {refused:?}"))
    })?;
    Ok((stream, batch))
}

/// The rest of the read on `stream` after `batch`, a frame's credit granted per batch after `pause`: how many batches it took.
async fn rest_of(
    client: &mut RoleClient,
    stream: u64,
    mut batch: Batch,
    pause: Duration,
) -> TestResult<usize> {
    let mut batches = 1;
    while !batch.last {
        compio::time::sleep(pause).await;
        client.send_on(stream, &Credit::new(1)).await?;
        batch = next_of(client).await?.ok().context("a batch")?;
        batches += 1;
    }
    Ok(batches)
}

/// A read at snapshot level holds its partition's version from the first frame that says it: one Scan of that version, of limit 1
/// over several ranges of one key, which takes no further frame within the 10 s of `scan_stall_ms / 3` at the default and stays open
/// while the read waits on its client; it is cancelled with the read's end.
#[test]
fn test_a_snapshot_read_holds_its_version_from_its_first_frame() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        hold_rows(partition, 3, 10)?;
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (stream, batch) = first_batch(&mut client, read).await?;
        compio::time::sleep(Duration::from_millis(50)).await;
        let pins = partition.pins();
        assert_eq!(pins.len(), 1, "{pins:?}");
        let pin = pins.first().context("a pin")?;
        assert_eq!(
            (pin.at, pin.read_version, pin.epoch, pin.limit),
            (ScanAt::ReadVersion, version()?, 0, 1)
        );
        assert_eq!(partition.credited(pin.connection, pin.stream), 0);
        assert!(!partition.cancelled(pin.connection, pin.stream));
        assert_eq!(
            rest_of(&mut client, stream, batch, Duration::ZERO).await?,
            3
        );
        within(Duration::from_secs(5), "the pin's cancel", async {
            while !partition.cancelled(pin.connection, pin.stream) {
                compio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(())
        })
        .await
    })
}

/// A short read at latest level, whose version it chose as it began, holds none: the partition keeps it for a quarter of
/// `read_retain_ms` without being asked.
#[test]
fn test_a_short_latest_read_holds_no_version() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        hold_rows(partition, 1, 10)?;
        let mut client = played.client().await?;
        let read = read_of(
            &Pattern::of(None, Some(P), None),
            true,
            ReadLevel::Latest,
            Vec::new(),
        )?;
        assert_eq!(answer_on(&mut client, &read).await?.whole()?.len(), 1);
        assert!(partition.pins().is_empty());
        Ok(())
    })
}

/// A read of two partitions answering one version holds it at each.
#[test]
fn test_each_partition_holds_its_version() -> TestResult {
    run(async {
        let played = Played::start(2, read_times_of(None, None)).await?;
        for (number, base) in [(1, 10), (2, 100)] {
            hold_rows(played.partition(number)?, 1, base)?;
        }
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (stream, batch) = first_batch(&mut client, read).await?;
        compio::time::sleep(Duration::from_millis(20)).await;
        for number in [1, 2] {
            assert_eq!(
                played.partition(number)?.pins().len(),
                1,
                "partition {number}"
            );
        }
        assert_eq!(
            rest_of(&mut client, stream, batch, Duration::ZERO).await?,
            2
        );
        Ok(())
    })
}

/// A read that goes on taking frames past one held Scan's grants has the next opened, so its version stays held: a read paced at
/// 100 ms a frame over 12 frames outlasts the 750 ms a Scan holds at `scan_stall_ms` 150.
#[test]
fn test_a_read_going_on_holds_its_version_past_one_scans_grants() -> TestResult {
    run(async {
        let played = Played::start(1, quick()).await?;
        let partition = played.partition(1)?;
        partition.split_frames();
        hold_rows(partition, 12, 10)?;
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (stream, batch) = first_batch(&mut client, read).await?;
        let pause = Duration::from_millis(100);
        assert_eq!(rest_of(&mut client, stream, batch, pause).await?, 12);
        assert!(
            partition.pins().len() >= 2,
            "{} held Scans",
            partition.pins().len()
        );
        Ok(())
    })
}

/// A read that stopped taking frames holds its version no longer than the Scan holding it when it stopped: past that, no further
/// Scan opens, however long it waits.
#[test]
fn test_a_stopped_read_opens_no_further_scan() -> TestResult {
    run(async {
        let played = Played::start(1, quick()).await?;
        let partition = played.partition(1)?;
        hold_rows(partition, 2, 10)?;
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (stream, batch) = first_batch(&mut client, read).await?;
        compio::time::sleep(ONE_SCAN * 2).await;
        let held = partition.pins().len();
        compio::time::sleep(ONE_SCAN * 2).await;
        assert_eq!(partition.pins().len(), held);
        assert!(held <= 2, "{held} held Scans");
        assert_eq!(
            rest_of(&mut client, stream, batch, Duration::ZERO).await?,
            2
        );
        Ok(())
    })
}

/// A read asking one partition twice at one version holds it once: a resumed reverse read reads its object's home for the referrer
/// set, and again for the IDs of its resume key's terms, the home not among the partitions it then reads.
#[test]
fn test_a_partition_asked_twice_at_one_version_is_held_once() -> TestResult {
    run(async {
        let played = Played::start(2, read_times_of(None, None)).await?;
        let (home, member) = (played.partition(1)?, played.partition(2)?);
        let object = played.owned_by(1, "o")?;
        let subjects = [played.owned_by(2, "s")?, played.owned_by(2, "t")?];
        let (o, p) = (id(2)?, id(1)?);
        for partition in [home, member] {
            partition.term(&object, o);
            partition.term(P, p);
        }
        let group = ultragraphdb_core_ids::BucketGroup::of(
            ultragraphdb_store_quads::iri_bucket(
                crate::utils::played_cluster::TENANT,
                &subjects[0],
            )
            .context("a bucket")?,
        );
        let mut set = ultragraphdb_store_quads::ReferrerSet::new();
        set.set(
            group,
            std::num::NonZeroU64::MIN,
            ultragraphdb_core_ids::Epoch::new(1)?,
        );
        let mut value = Vec::new();
        set.encode(&mut value);
        let key = ultragraphdb_core_keys::QuadKey::Referrers {
            tenant: crate::utils::played_cluster::TENANT,
            entity: o,
        };
        home.put(key.to_bytes(), value);
        for (at, subject) in (3u64..).zip(&subjects) {
            let s = id(at)?;
            member.term(subject, s);
            member.quads(
                ultragraphdb_core_keys::Graph::Default,
                s,
                p,
                &[ultragraphdb_core_terms::EncodedTerm::id(o)],
            );
        }
        let pattern = Pattern::of(
            None,
            Some(P),
            Some(crate::utils::played_answers::iri(&object)),
        );
        let mut client = played.client().await?;
        let first = answer_on(&mut client, &snapshot(&pattern)?.with_limit(1)).await?;
        assert_eq!(first.whole()?.len(), 1);
        let resume = first.batches.last().context("a batch")?.resume.clone();
        let held = home.pins().len();
        let resumed = read_of(&pattern, true, ReadLevel::Snapshot, resume)?
            .with_read_version(version()?)
            .with_epoch(crate::utils::played_cluster::EPOCH);
        assert_eq!(answer_on(&mut client, &resumed).await?.whole()?.len(), 1);
        let gates = home.scans().iter().filter(|scan| !scan.is_pin()).count();
        assert!(
            gates >= 4,
            "the home is asked twice per read: {gates} Scans"
        );
        assert_eq!(home.pins().len(), held + 1);
        Ok(())
    })
}
