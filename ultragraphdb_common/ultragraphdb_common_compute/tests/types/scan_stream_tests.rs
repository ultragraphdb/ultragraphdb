/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Scan's stream (`PROTOCOL.md` §13.13, §13.14, §11.12): a partition's refusal comes back as it was sent, a stream given up before
//! its end is cancelled, and a read given up while its partition stays silent leaves the connection watched, which closes once
//! the partition has been silent past its bound.

use std::time::Duration;

use ultragraphdb_common_compute::{ReadTimes, read_times_of};
use ultragraphdb_protocol_wire::{Cancel, ErrorCode, ReadLevel};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played_reads::played_cluster::{PlayedCluster, run};
use crate::played_reads::played_data::{
    Placed, first_bucket, forward_key, id_in, ids_list, name, place, subject_predicates_key,
    tenant_over,
};
use crate::played_reads::played_partition::{PlayedPartition, Script};
use crate::played_reads::read_answer::{Pattern, answer_on};

const SUBJECT: &str = "http://ex.org/s";
const OTHER_SUBJECT: &str = "http://ex.org/t";
const PREDICATE: &str = "http://ex.org/p";
const OBJECTS: [&str; 2] = ["http://ex.org/o1", "http://ex.org/o2"];

/// A cluster of one partition reading by `times`, holding `<s> <p>` listing two objects, and its tenant.
async fn one_list(times: ReadTimes) -> TestResult<(PlayedCluster, u64, Placed)> {
    let cluster = PlayedCluster::start(1, times).await?;
    let tenant = tenant_over(1, 1)?;
    let placed = place(
        cluster.partition(1)?,
        tenant,
        (first_bucket(tenant), 1),
        (SUBJECT, PREDICATE),
        &OBJECTS,
    )?;
    Ok((cluster, tenant, placed))
}

/// A partition's refusal of a Scan reaches the read as the partition sent it: its code and its detail.
#[test]
fn test_a_partitions_refusal_comes_back_as_it_was_sent() -> TestResult {
    run(async {
        let (cluster, tenant, placed) = one_list(read_times_of(None, None)).await?;
        let key = forward_key(tenant, placed.predicate, placed.subject);
        cluster
            .partition(1)?
            .script(key, Script::Refuse(ErrorCode::TooOld, 42));
        let read = Pattern::of(Some(SUBJECT), Some(PREDICATE), None).read(
            tenant,
            true,
            ReadLevel::Latest,
        )?;
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        let refused = answer.refusal()?;
        assert_eq!((refused.code, refused.detail), (ErrorCode::TooOld, 42));
        Ok(())
    })
}

/// A read stopped at its limit before its Scan's last frame cancels that Scan, and only that one: every Scan that ended needs
/// none.
#[test]
fn test_a_scan_given_up_before_its_end_is_cancelled() -> TestResult {
    run(async {
        let (cluster, tenant, placed) = one_list(read_times_of(None, None)).await?;
        let part = cluster.partition(1)?;
        let other = id_in(first_bucket(tenant), 9)?;
        name(part, tenant, "http://ex.org/q", other);
        let object = placed.objects.first().copied().context("an object")?;
        part.put(
            forward_key(tenant, other, placed.subject),
            ids_list(&[object]),
        );
        part.put(
            subject_predicates_key(tenant, placed.subject),
            ids_list(&[placed.predicate, other]),
        );
        let read = Pattern::of(Some(SUBJECT), None, None)
            .read(tenant, true, ReadLevel::Latest)?
            .with_limit(2);
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        assert_eq!(answer.column(1), OBJECTS);
        assert!(!answer.resume().is_empty(), "a row is left");
        // The Cancel leaves as the read's Scans are dropped, after its last batch: waited for, a bounded while.
        for _ in 0..200 {
            if part.cancels() > 0 {
                break;
            }
            compio::time::sleep(Duration::from_millis(10)).await;
        }
        compio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            part.cancels(),
            1,
            "the Scan of both lists, left after the first"
        );
        Ok(())
    })
}

/// A read cancelled while its partition is silent, past half the partition's bound, leaves the connection watched: silent past
/// the bound from its request, the connection closes, and the next read dials anew (`PROTOCOL.md` §11.12, *Errors*).
#[test]
fn test_a_read_given_up_on_a_silent_partition_closes_its_connection() -> TestResult {
    run(async {
        // `scan_stall_ms` 1 s: a partition is hung once silent 2 s past a request, and a read given up past 1 s leaves it watched.
        let times = read_times_of(Some(1_000), None);
        let (cluster, tenant, placed) = one_list(times).await?;
        let part = cluster.partition(1)?;
        silent_and_other(part, tenant, &placed)?;
        let other = Pattern::of(Some(OTHER_SUBJECT), Some(PREDICATE), None).read(
            tenant,
            true,
            ReadLevel::Latest,
        )?;
        // A read first, so the placement, the members and the connection are held and the silent Scan is asked at once.
        let mut client = cluster.client().await?;
        assert_eq!(answer_on(&mut client, &other).await?.column(0), OBJECTS);
        let read = Pattern::of(Some(SUBJECT), Some(PREDICATE), None).read(
            tenant,
            true,
            ReadLevel::Latest,
        )?;
        client.send_only(&read).await?;
        let stream = client.last_stream();
        compio::time::sleep(Duration::from_millis(1_500)).await;
        client.send_on(stream, &Cancel).await?;
        compio::time::sleep(Duration::from_millis(1_100)).await;
        assert_eq!(part.connections(), 1);
        let answer = answer_on(&mut client, &other).await?;
        assert_eq!(answer.column(0), OBJECTS);
        assert_eq!(
            part.connections(),
            2,
            "the watched connection closed, and the next read dialled"
        );
        Ok(())
    })
}

/// Has `part` leave the list of `<s> <p>` unanswered, and hold `<t> <p>` listing the same objects.
fn silent_and_other(part: &PlayedPartition, tenant: u64, placed: &Placed) -> TestResult {
    let key = forward_key(tenant, placed.predicate, placed.subject);
    part.script(key, Script::Silent);
    let other = id_in(first_bucket(tenant), 8)?;
    name(part, tenant, OTHER_SUBJECT, other);
    part.put(
        forward_key(tenant, placed.predicate, other),
        ids_list(&placed.objects),
    );
    Ok(())
}
