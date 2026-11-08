/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The times a read holds its versions by (`PROTOCOL.md` §11.9): a read choosing its version as it begins holds it from a quarter
//! of `read_retain_ms` on, and a version held takes a frame of its Scan every third of `scan_stall_ms`.

use std::time::Duration;

use ultragraphdb_common_compute::ReadTimes;
use ultragraphdb_protocol_wire::{ComputeRead, ReadLevel};
use ultragraphdb_tool_bdd::TestResult;

use crate::played::played_cluster::{PlayedCluster, run};
use crate::played::played_data::{OneList, forward};
use crate::played::played_partition::PlayedPartition;
use crate::played::played_read::{Bound, read, read_of, snapshot_of};

/// A read of `list` by `asked` on a cluster reading by `times`, its list's Scan answered `delay` after it arrives: the partition.
async fn slow_read(
    times: ReadTimes,
    delay: Duration,
    asked: impl Fn(&Bound) -> TestResult<ComputeRead>,
) -> TestResult<PlayedPartition> {
    let played = PlayedCluster::start(1, times).await?;
    let partition = played.partition(1)?.clone();
    let list = OneList::new("subject", "predicate", 2)?;
    list.hold_in(&partition);
    partition.delay(forward(&list.predicate, &list.subject), delay);
    let bound = Bound {
        subject: Some(list.subject.iri.clone()),
        predicate: Some(list.predicate.iri.clone()),
        object: None,
    };
    let mut client = played.client().await?;
    let answer = read(&mut client, &asked(&bound)?).await?.answered()?;
    assert_eq!(answer.column(0), list.object_iris());
    Ok(partition)
}

/// A read at latest level holds no version for as long as a quarter of `read_retain_ms`.
#[test]
fn test_a_read_choosing_its_version_holds_none_while_it_is_short() -> TestResult {
    run(async {
        let retain = Duration::from_secs(60);
        let times = ReadTimes::new(Duration::from_secs(2), retain);
        let lasting = Duration::from_millis(300);
        assert!(lasting < retain / 4);
        let latest = |bound: &Bound| read_of(bound, true, ReadLevel::Latest, Vec::new());
        let partition = slow_read(times, lasting, latest).await?;
        assert_eq!(partition.pins(), Vec::new());
        Ok(())
    })
}

/// A read at latest level lasting past a quarter of `read_retain_ms` holds the version it chose.
#[test]
fn test_a_read_choosing_its_version_holds_it_once_it_lasts() -> TestResult {
    run(async {
        let retain = Duration::from_millis(400);
        let times = ReadTimes::new(Duration::from_secs(2), retain);
        let lasting = Duration::from_millis(600);
        assert!(lasting > retain / 4 * 2);
        let latest = |bound: &Bound| read_of(bound, true, ReadLevel::Latest, Vec::new());
        let partition = slow_read(times, lasting, latest).await?;
        assert!(!partition.pins().is_empty());
        Ok(())
    })
}

/// A version held takes a frame of its Scan every third of `scan_stall_ms`, so a read lasting a few of those grants a few frames.
#[test]
fn test_a_held_version_takes_a_frame_every_third_of_the_stall() -> TestResult {
    run(async {
        let stall = Duration::from_millis(400);
        let times = ReadTimes::new(stall, Duration::from_secs(60));
        let lasting = Duration::from_millis(500);
        let partition = slow_read(times, lasting, snapshot_of).await?;
        let granted = partition.credits_on(&partition.pins());
        let expected = u32::try_from(lasting.as_millis() / (stall / 3).as_millis())?;
        assert!(
            (1..=expected * 2).contains(&granted),
            "{granted} grants, about {expected} expected"
        );
        Ok(())
    })
}
