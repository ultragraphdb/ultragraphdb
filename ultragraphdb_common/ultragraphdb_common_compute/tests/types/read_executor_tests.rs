/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The executor a session hands its reads to (design D3): a read handed over, its frames, its end and its refusal taken
//! back, room and Cancel forward; rows partition by partition (`PROTOCOL.md` §9.1); the step bound (laws L5.9 and L5.10 of
//! `notes/laws.md`). Cases C5.1 to C5.3, C5.30, C5.31, C5.38 and C5.45 of `notes/corner-cases.md`.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_common_compute::ReadExecutor;
use ultragraphdb_common_role::OpenRequests;
use ultragraphdb_core_keys::{Graph, Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{ErrorCode, MAX_FRAME_FLOOR, MAX_FRAME_FLOOR_U32};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::apart::apart_cluster::{ApartCluster, links_holding_streams, run, times};
use crate::apart::apart_executor::{
    STREAM, bound, executed, gives_within, next_of, pattern_of, read_frame, snapshot, take,
};
use crate::apart::apart_partition::Script;
use crate::apart::apart_reads::{
    Lists, P, ReadAnswer, TENANT, TWO_BY_TWO, hold_lists, rows_listed, rows_of,
};

/// How long a test waits to see that nothing comes.
const QUIET: Duration = Duration::from_millis(200);

/// One partition holding `lists` of `<p>`, framed per key when `per_key`, and an executor over a core of the cluster.
async fn one_holding(lists: Lists<'_>, per_key: bool) -> TestResult<(ApartCluster, ReadExecutor)> {
    let cluster = ApartCluster::start(1).await?;
    let partition = cluster.partition(1)?;
    let _p = hold_lists(partition, (Graph::Default, P), lists, 100)?;
    if per_key {
        partition.frame_per_key();
    }
    let executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(1))));
    Ok((cluster, executor))
}

/// C5.2: a read of one subject on one partition answers one batch flagged `LAST`, holding its rows, on the session's stream.
#[test]
fn test_a_read_of_one_subject_answers_one_batch() -> TestResult {
    run(async {
        let (_cluster, mut executor) = one_holding(TWO_BY_TWO, false).await?;
        let read = bound(pattern_of(&read_frame(
            (false, Some("http://ex.org/s1"), Some(P)),
            0,
            Vec::new(),
        )?)?);
        let (answer, sizes) = executed(&mut executor, read).await?;
        // The pattern binds the subject and the predicate: the columns are the object and the reifier.
        let rows = vec![
            vec![Some("http://ex.org/o1".to_string()), None],
            vec![Some("http://ex.org/o2".to_string()), None],
        ];
        assert_eq!(answer.whole()?, rows.as_slice());
        assert_eq!(sizes.len(), 1);
        assert!(answer.resume.is_empty());
        Ok(())
    })
}

/// C5.1: a read matching nothing answers one batch flagged `LAST`, `rows` 0.
#[test]
fn test_a_read_matching_nothing_answers_one_empty_batch() -> TestResult {
    run(async {
        let (_cluster, mut executor) = one_holding(TWO_BY_TWO, false).await?;
        let read = bound(pattern_of(&read_frame(
            (false, Some("http://ex.org/nobody"), Some(P)),
            0,
            Vec::new(),
        )?)?);
        let (answer, sizes) = executed(&mut executor, read).await?;
        assert!(answer.whole()?.is_empty());
        assert_eq!((answer.batches, sizes.len()), (1, 1));
        Ok(())
    })
}

/// C5.3 and `PROTOCOL.md` §9.1: a read over two partitions returns every row of partition 1, then every row of partition 2,
/// although partition 2's keys sort below partition 1's (its predicate's ID is the lower), so key order would put them first.
#[test]
fn test_rows_come_partition_by_partition() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(2).await?;
        const ONE: Lists<'static> = &[("http://ex.org/z", &["http://ex.org/z1"])];
        const TWO: Lists<'static> =
            &[("http://ex.org/a", &["http://ex.org/a1", "http://ex.org/a2"])];
        let _high = hold_lists(cluster.partition(1)?, (Graph::Default, P), ONE, 500)?;
        let _low = hold_lists(cluster.partition(2)?, (Graph::Default, P), TWO, 100)?;
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(1))));
        let read = snapshot(
            pattern_of(&read_frame((false, None, Some(P)), 0, Vec::new())?)?,
            MAX_FRAME_FLOOR_U32,
        );
        let (answer, _sizes) = executed(&mut executor, read).await?;
        let mut expected = rows_listed(ONE);
        expected.extend(rows_listed(TWO));
        assert_eq!(answer.whole()?, expected.as_slice());
        Ok(())
    })
}

/// Four objects whose IRIs each take a third of the client's frame: no batch holds more than two of them.
fn long_objects() -> Vec<String> {
    (1..=4)
        .map(|at| format!("http://ex.org/{at}/{}", "o".repeat(MAX_FRAME_FLOOR / 3)))
        .collect()
}

/// C5.30, C5.31 and L5.7: the read builds its next batch only once the session gives room for it, and every batch is within
/// the client's `max_frame`.
#[test]
fn test_no_batch_is_built_before_its_room() -> TestResult {
    run(async {
        let objects = long_objects();
        let named: Vec<&str> = objects.iter().map(String::as_str).collect();
        let lists = [("http://ex.org/s1", named.as_slice())];
        let (_cluster, mut executor) = one_holding(&lists, false).await?;
        let mut read = bound(pattern_of(&read_frame(
            (false, None, Some(P)),
            0,
            Vec::new(),
        )?)?);
        read = read.with_max_frame(MAX_FRAME_FLOOR_U32);
        executor.start(STREAM, read);
        let mut answer = ReadAnswer::default();
        let (_stream, first) = next_of(&mut executor).await?;
        assert!(first.len() <= MAX_FRAME_FLOOR);
        assert!(
            !take(&mut answer, &first)?,
            "the first batch is not the last"
        );
        assert!(
            !gives_within(&mut executor, QUIET).await,
            "no batch is built before its room"
        );
        // Each pass gives room for one batch and takes it, until the last (Ultra rule 2: four objects, at most four batches).
        for _ in 0..4 {
            executor.ready(STREAM);
            let (_stream, frame) = next_of(&mut executor).await?;
            assert!(frame.len() <= MAX_FRAME_FLOOR);
            if take(&mut answer, &frame)? {
                break;
            }
        }
        let pairs: Vec<(&str, &str)> = named
            .iter()
            .map(|object| ("http://ex.org/s1", *object))
            .collect();
        assert_eq!(answer.whole()?, rows_of(&pairs).as_slice());
        assert!(answer.batches >= 2, "{} batches", answer.batches);
        Ok(())
    })
}

/// C5.38 and L5.8: once the session cancels a read, no frame of it is handed out, and its Scan's stream is let go.
#[test]
fn test_a_cancelled_read_hands_out_nothing_more() -> TestResult {
    run(async {
        let objects = long_objects();
        let named: Vec<&str> = objects.iter().map(String::as_str).collect();
        let lists = [("http://ex.org/s1", named.as_slice())];
        let cluster = ApartCluster::start(1).await?;
        let _p = hold_lists(cluster.partition(1)?, (Graph::Default, P), &lists, 100)?;
        let core = cluster.core(times(Duration::from_secs(1)));
        let mut executor = ReadExecutor::new(Rc::clone(&core));
        let read = bound(pattern_of(&read_frame(
            (false, None, Some(P)),
            0,
            Vec::new(),
        )?)?)
        .with_max_frame(MAX_FRAME_FLOOR_U32);
        executor.start(STREAM, read);
        let _first = next_of(&mut executor).await?;
        executor.ready(STREAM);
        executor.cancel(STREAM);
        assert!(
            !gives_within(&mut executor, QUIET).await,
            "nothing of a cancelled read is handed out"
        );
        let (_links, holding) = links_holding_streams(&core).await;
        assert_eq!(holding, 0, "the cancelled read's Scans are let go");
        Ok(())
    })
}

/// A read whose resume key continues another pattern is refused with Error 1 naming the resume key, as the stream's last
/// frame (`PROTOCOL.md` §11.8).
#[test]
fn test_a_refusal_is_the_stream_s_last_frame() -> TestResult {
    run(async {
        let (_cluster, mut executor) = one_holding(TWO_BY_TWO, false).await?;
        let first = bound(pattern_of(&read_frame(
            (false, None, Some(P)),
            1,
            Vec::new(),
        )?)?);
        let (answer, _sizes) = executed(&mut executor, first).await?;
        let key = answer.resume.clone();
        assert!(!key.is_empty(), "a limited read ends with a key");
        let other = bound(pattern_of(&read_frame(
            (false, Some("http://ex.org/s2"), Some(P)),
            1,
            key,
        )?)?);
        let (refused, sizes) = executed(&mut executor, other).await?;
        let (code, _detail, message) = refused.refused.context("a refusal")?;
        assert_eq!(code, ErrorCode::Protocol);
        assert!(message.contains("resume key"), "{message}");
        assert_eq!(sizes.len(), 1);
        Ok(())
    })
}

/// A read started again on a stream whose read ended runs to its own end: what the read before sent late is not taken for
/// it (design note, section 1).
#[test]
fn test_a_read_started_again_on_its_stream_runs_to_its_end() -> TestResult {
    run(async {
        let (_cluster, mut executor) = one_holding(TWO_BY_TWO, false).await?;
        let refused = bound(pattern_of(&read_frame(
            (false, None, Some(P)),
            0,
            vec![1, 2, 3],
        )?)?);
        executor.start(STREAM, refused);
        let (_stream, frame) = next_of(&mut executor).await?;
        let mut answer = ReadAnswer::default();
        assert!(take(&mut answer, &frame)?);
        assert!(answer.refused.is_some(), "a key of three bytes is refused");
        let again = bound(pattern_of(&read_frame(
            (false, None, Some(P)),
            0,
            Vec::new(),
        )?)?);
        let (answer, _sizes) = executed(&mut executor, again).await?;
        assert_eq!(answer.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        Ok(())
    })
}

/// An executor counting its reads among the open requests holds one while its read runs and none once it ended.
#[test]
fn test_an_executor_with_open_counts_each_read_until_it_ends() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let _p = hold_lists(partition, (Graph::Default, P), TWO_BY_TWO, 100)?;
        let lists = key_prefix(Keyspace::Forward, false, TENANT, Some(Graph::Default), None);
        partition.script(lists.clone(), Script::Silent, 1);
        let open = OpenRequests::default();
        let mut executor =
            ReadExecutor::new(cluster.core(times(Duration::from_secs(2)))).with_open(open.clone());
        let read = bound(pattern_of(&read_frame(
            (false, None, Some(P)),
            0,
            Vec::new(),
        )?)?);
        executor.start(STREAM, read);
        partition.until_asked(&lists).await?;
        assert_eq!(open.count(), 1, "the read is open");
        executor.cancel(STREAM);
        assert_eq!(open.count(), 0, "a cancelled read is open no longer");
        Ok(())
    })
}

/// C5.45 and L5.10: a read whose every Entries frame is ready gives the core back after each step, so a task that runs
/// whenever it is let sees the core's step count move by at most one between two of its runs.
#[test]
fn test_a_read_gives_its_core_back_after_each_step() -> TestResult {
    run(async {
        let objects: Vec<String> = (1..=24).map(|at| format!("http://ex.org/o{at}")).collect();
        let named: Vec<&str> = objects.iter().map(String::as_str).collect();
        let lists: Vec<(String, Vec<&str>)> = (1..=12)
            .map(|at| (format!("http://ex.org/s{at}"), named.clone()))
            .collect();
        let listed: Vec<(&str, &[&str])> = lists
            .iter()
            .map(|(subject, objects)| (subject.as_str(), objects.as_slice()))
            .collect();
        let (cluster, _unused) = one_holding(&listed, true).await?;
        let core = cluster.core(times(Duration::from_secs(1)));
        let mut executor = ReadExecutor::new(Rc::clone(&core));
        let watched = Rc::clone(&core);
        let widest = Rc::new(Cell::new(0u64));
        let seen = Rc::clone(&widest);
        let done = Rc::new(Cell::new(false));
        let stop = Rc::clone(&done);
        let probe = spawn(async move {
            let mut last = watched.steps();
            // Each pass yields once and looks; the read's end stops it (Ultra rule 2: the test's own wait bounds it).
            while !stop.get() {
                yield_now().await;
                let now = watched.steps();
                seen.set(seen.get().max(now.saturating_sub(last)));
                last = now;
            }
        });
        let read = bound(pattern_of(&read_frame(
            (false, None, Some(P)),
            0,
            Vec::new(),
        )?)?);
        let (answer, _sizes) = executed(&mut executor, read).await?;
        done.set(true);
        drop(probe);
        assert_eq!(answer.whole()?.len(), 12 * 24);
        assert!(
            core.steps() >= 12,
            "one step at least per Entries frame: {}",
            core.steps()
        );
        assert!(
            widest.get() <= 1,
            "the probe saw {} steps at once",
            widest.get()
        );
        Ok(())
    })
}

/// Lets every other ready task of the core run once.
async fn yield_now() {
    let mut yielded = false;
    std::future::poll_fn(|context| {
        if yielded {
            return std::task::Poll::Ready(());
        }
        yielded = true;
        context.waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await;
}

/// A read whose task was never handed a frame's room within the test fails the test rather than holding it.
#[test]
fn test_the_executor_s_next_waits_while_no_read_has_a_frame() -> TestResult {
    run(async {
        let (_cluster, mut executor) = one_holding(TWO_BY_TWO, false).await?;
        if gives_within(&mut executor, QUIET).await {
            return Err(TestError::new("an executor holding no read gave a frame"));
        }
        Ok(())
    })
}
