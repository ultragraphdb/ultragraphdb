/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of a client's read run on its connection's core (quad-reads, "A read's step is bounded by one frame",
//! "Compute runs each read on one core", "A read's batches fit the client's frame", "Compute times a partition that stops
//! answering", "Compute executes quad-pattern reads over storage"), through the executor a client's session hands its reads
//! to, over played partitions. The scenario plays the client's connection: it gives a read room for its next batch as the
//! connection's credit allows, as the connection's outbound queue tells a session (`PROTOCOL.md` §11.3).

use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use cucumber::{then, when};
use ultragraphdb_common_compute::{ComputeCore, ReadExecutor, ReadRequest};
use ultragraphdb_core_keys::{Graph, Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{
    BodyView, ErrorCode, FLAG_LAST, FrameView, HeaderView, MAX_FRAME_FLOOR, MAX_FRAME_FLOOR_U32,
    MAX_FRAME_SIZE, ResultView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::Steps;
use crate::apart::apart_cluster::{ApartCluster, CLUSTER, links_holding_streams, run, times};
use crate::apart::apart_executor::{
    STREAM, executed, gives_within, next_of, pattern_of, read_frame, snapshot, take,
};
use crate::apart::apart_partition::Script;
use crate::apart::apart_reads::{Lists, P, ReadAnswer, TENANT, hold_lists, rows_of};

/// How long a scenario waits to see that nothing comes.
const QUIET: Duration = Duration::from_millis(200);

/// One partition's lists: each subject's IRI and its objects' IRIs.
type Held = Vec<(String, Vec<String>)>;

/// Runs a scenario's `work` on a runtime of its own, and notes that it ran.
fn scenario<F: Future<Output = TestResult>>(w: &mut Steps, work: F) -> TestResult {
    run(work)?;
    w.ran = true;
    Ok(())
}

/// `count` subjects of `<p>` in partition `partition`, each with `objects` objects.
fn lists_of(partition: u32, count: usize, objects: usize) -> Held {
    (0..count)
        .map(|subject| {
            (
                format!("http://ex.org/{partition}/s{subject}"),
                (0..objects)
                    .map(|object| format!("http://ex.org/{partition}/o{object}"))
                    .collect(),
            )
        })
        .collect()
}

/// Has `cluster`'s partitions 1 to `lists.len()` hold `lists`, each partition's IDs drawn above the next one's, so keys sort
/// across partitions the other way from the partitions' order; framed per key when `per_key`.
fn hold_all(cluster: &ApartCluster, lists: &[Held], per_key: bool) -> TestResult {
    let count = u64::try_from(lists.len())?;
    for (at, held) in lists.iter().enumerate() {
        let partition = u32::try_from(at)? + 1;
        let objects: Vec<Vec<&str>> = held
            .iter()
            .map(|(_, objects)| objects.iter().map(String::as_str).collect())
            .collect();
        let borrowed: Vec<(&str, &[&str])> = held
            .iter()
            .zip(&objects)
            .map(|((subject, _), objects)| (subject.as_str(), objects.as_slice()))
            .collect();
        let played = cluster.partition(partition)?;
        let first = (count - u64::from(partition) + 1) * 1_000;
        let _p = hold_lists(played, (Graph::Default, P), &borrowed, first)?;
        if per_key {
            played.frame_per_key();
        }
    }
    Ok(())
}

/// The rows of `lists`, partition after partition.
fn rows_of_all(lists: &[Held]) -> Vec<Vec<Option<String>>> {
    lists
        .iter()
        .flat_map(|held| {
            held.iter().flat_map(|(subject, objects)| {
                objects
                    .iter()
                    .map(move |object| vec![Some(subject.clone()), Some(object.clone()), None])
            })
        })
        .collect()
}

/// A read-only read binding `<p>` alone, its batches within `max_frame`.
fn read_of_p(max_frame: u32) -> TestResult<ReadRequest> {
    Ok(snapshot(
        pattern_of(&read_frame((false, None, Some(P)), 0, Vec::new())?)?,
        max_frame,
    ))
}

/// Watches `core` from a task of its own that runs whenever it is let: the most steps it saw the core take between two of its
/// runs, read once `done` is set.
fn watch(core: &Rc<ComputeCore>, done: &Rc<Cell<bool>>) -> Rc<Cell<u64>> {
    let widest = Rc::new(Cell::new(0u64));
    let (seen, stop, watched) = (Rc::clone(&widest), Rc::clone(done), Rc::clone(core));
    spawn(async move {
        let mut last = watched.steps();
        // Each pass yields once and looks; `done` ends it.
        while !stop.get() {
            yield_now().await;
            let now = watched.steps();
            seen.set(seen.get().max(now.saturating_sub(last)));
            last = now;
        }
    })
    .detach();
    widest
}

/// Now, from the runtime's timer, whose first tick is now.
async fn now() -> std::time::Instant {
    compio::time::interval(Duration::from_secs(1)).tick().await
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

#[when(
    "a read merging rows of four partitions is under way on a core, and a Commit of another connection arrives on that core"
)]
fn a_commit_beside_a_merge(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let cluster = ApartCluster::start(4).await?;
        let lists: Vec<Held> = (1..=4).map(|partition| lists_of(partition, 4, 3)).collect();
        hold_all(&cluster, &lists, true)?;
        let core = cluster.core(times(Duration::from_secs(1)));
        let mut executor = ReadExecutor::new(Rc::clone(&core));
        executor.start(STREAM, read_of_p(MAX_FRAME_FLOOR_U32)?);
        // The read is under way: it took steps while this task let the core run.
        while core.steps() < 3 {
            yield_now().await;
        }
        // The Commit's relay: a task the core runs once it is ready, noting the steps the read took before it ran.
        let before = core.steps();
        let relayed = Rc::new(Cell::new(None));
        let (noted, watched) = (Rc::clone(&relayed), Rc::clone(&core));
        spawn(async move { noted.set(Some(watched.steps())) }).detach();
        let mut answer = ReadAnswer::default();
        // Each pass takes one batch: the rows of four partitions bound them.
        for _ in 0..64 {
            let (_stream, frame) = next_of(&mut executor).await?;
            if take(&mut answer, &frame)? {
                break;
            }
            executor.ready(STREAM);
        }
        assert_eq!(answer.whole()?, rows_of_all(&lists).as_slice());
        let at = relayed.get().context("the Commit was relayed")?;
        assert!(
            at.saturating_sub(before) <= 1,
            "the Commit waited {} steps",
            at.saturating_sub(before)
        );
        Ok(())
    })
}

#[then("the Commit is relayed to its partition before the read's next Result frame is built")]
fn a_commit_beside_a_merge_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a read's partitions each have ten Entries frames ready")]
fn ten_frames_ready(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let cluster = ApartCluster::start(2).await?;
        let lists: Vec<Held> = (1..=2)
            .map(|partition| lists_of(partition, 10, 2))
            .collect();
        hold_all(&cluster, &lists, true)?;
        let core = cluster.core(times(Duration::from_secs(1)));
        let done = Rc::new(Cell::new(false));
        let widest = watch(&core, &done);
        let mut executor = ReadExecutor::new(Rc::clone(&core));
        let (answer, _sizes) = executed(&mut executor, read_of_p(MAX_FRAME_FLOOR_U32)?).await?;
        done.set(true);
        assert_eq!(answer.whole()?, rows_of_all(&lists).as_slice());
        assert!(
            core.steps() >= 20,
            "{} steps for twenty frames and more",
            core.steps()
        );
        assert!(
            widest.get() <= 1,
            "another task waited {} steps",
            widest.get()
        );
        Ok(())
    })
}

#[then(
    "the read consumes one frame, yields, and consumes the next only after every other ready task of the core has run a step"
)]
fn ten_frames_ready_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a read fans out over three partitions")]
fn a_fan_out(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let cluster = ApartCluster::start(3).await?;
        let lists: Vec<Held> = (1..=3).map(|partition| lists_of(partition, 2, 2)).collect();
        hold_all(&cluster, &lists, false)?;
        let core = cluster.core(times(Duration::from_secs(1)));
        let mut executor = ReadExecutor::new(Rc::clone(&core));
        let (answer, _sizes) = executed(&mut executor, read_of_p(MAX_FRAME_FLOOR_U32)?).await?;
        assert_eq!(answer.whole()?, rows_of_all(&lists).as_slice());
        // The read took its Scans on its core's one connection to each partition, and its steps on that core.
        for partition in 1..=3 {
            assert_eq!(cluster.partition(partition)?.connections(), 1);
            assert!(!cluster.partition(partition)?.scans().is_empty());
        }
        assert!(core.steps() > 0);
        Ok(())
    })
}

#[then(
    "its Scans, its merge and its Result frames all run on the core its client's connection arrived on"
)]
fn a_fan_out_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read leaves its subject and object unbound over two partitions that each hold matching rows"
)]
fn partition_by_partition(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let cluster = ApartCluster::start(2).await?;
        let lists: Vec<Held> = (1..=2).map(|partition| lists_of(partition, 2, 2)).collect();
        hold_all(&cluster, &lists, false)?;
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(1))));
        let (answer, _sizes) = executed(&mut executor, read_of_p(MAX_FRAME_FLOOR_U32)?).await?;
        assert_eq!(answer.whole()?, rows_of_all(&lists).as_slice());
        Ok(())
    })
}

#[then("compute returns every row of partition 1 and then every row of partition 2")]
fn partition_by_partition_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// `count` objects, each an IRI taking a third of a client's frame of the floor.
fn third_of_a_frame(count: usize) -> Vec<String> {
    (1..=count)
        .map(|at| format!("http://ex.org/{at}/{}", "o".repeat(MAX_FRAME_FLOOR / 3)))
        .collect()
}

/// One partition holding `<s1>`'s `objects` of `<p>`, and an executor over a core of the cluster.
async fn holding_objects(objects: &[String]) -> TestResult<(ApartCluster, ReadExecutor)> {
    let cluster = ApartCluster::start(1).await?;
    let named: Vec<&str> = objects.iter().map(String::as_str).collect();
    let lists: Lists<'_> = &[("http://ex.org/s1", named.as_slice())];
    let _p = hold_lists(cluster.partition(1)?, (Graph::Default, P), lists, 100)?;
    let executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(1))));
    Ok((cluster, executor))
}

#[when("a client whose Hello names `max_frame` 524,288 reads more rows than one such frame holds")]
fn batches_at_the_floor(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let objects = third_of_a_frame(5);
        let (_cluster, mut executor) = holding_objects(&objects).await?;
        let (answer, sizes) = executed(&mut executor, read_of_p(MAX_FRAME_FLOOR_U32)?).await?;
        let pairs: Vec<(&str, &str)> = objects
            .iter()
            .map(|object| ("http://ex.org/s1", object.as_str()))
            .collect();
        assert_eq!(answer.whole()?, rows_of(&pairs).as_slice());
        assert!(sizes.len() >= 2, "{sizes:?}");
        // 524,288: the floor of a client's `max_frame` (`PROTOCOL.md` §11.9).
        assert!(sizes.iter().all(|size| *size <= 524_288), "{sizes:?}");
        Ok(())
    })
}

#[then("every Result frame it receives is at most 524,288 bytes, and every row arrives")]
fn batches_at_the_floor_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client whose Hello names `max_frame` 524,288 reads a row holding a literal of 600,000 bytes"
)]
fn a_row_past_the_frame(w: &mut Steps) -> TestResult {
    scenario(w, async {
        // The row's object stands for the literal: a term whose text takes 600,000 bytes, which no batch of 524,288 holds.
        let objects = vec![format!("http://ex.org/{}", "l".repeat(600_000))];
        let (_cluster, mut executor) = holding_objects(&objects).await?;
        let (answer, _sizes) = executed(&mut executor, read_of_p(MAX_FRAME_FLOOR_U32)?).await?;
        let (code, detail, _message) = answer.refused.context("a refusal")?;
        assert_eq!((code, detail), (ErrorCode::TooLarge, 524_288));
        Ok(())
    })
}

#[then("the read is refused with code 6, `detail` 524,288")]
fn a_row_past_the_frame_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read stops at its limit, and its rows fill the last batch so that its resume key does not fit"
)]
fn a_key_alone(w: &mut Steps) -> TestResult {
    scenario(w, async {
        // Each object takes three fifths of a frame of the floor: its row fits a batch, and its key, which holds it as text,
        // does not fit beside it.
        let objects: Vec<String> = (1..=2)
            .map(|at| format!("http://ex.org/{at}/{}", "o".repeat(MAX_FRAME_FLOOR * 3 / 5)))
            .collect();
        let (_cluster, mut executor) = holding_objects(&objects).await?;
        let limited = snapshot(
            pattern_of(&read_frame((false, None, Some(P)), 1, Vec::new())?)?,
            MAX_FRAME_FLOOR_U32,
        );
        executor.start(STREAM, limited);
        let (_stream, first) = next_of(&mut executor).await?;
        assert_eq!(
            HeaderView::new(&first)?.flags() & FLAG_LAST,
            0,
            "the batch of the row is not the last"
        );
        executor.ready(STREAM);
        let (_stream, last) = next_of(&mut executor).await?;
        let view = FrameView::validate(&last, CLUSTER, MAX_FRAME_SIZE)?;
        let result = ResultView::from_frame(&view)?;
        assert_ne!(view.header().flags() & FLAG_LAST, 0);
        assert_eq!(result.rows(), 0);
        assert!(!result.resume().is_empty());
        Ok(())
    })
}

#[then(
    "the stream's last batch holds `rows` 0 and the key, spending one credit, and the batch before it is not flagged `LAST`"
)]
fn a_key_alone_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client's Welcome names `initial_credit` 4, and a read's answer takes more frames")]
fn four_frames_of_credit(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let objects = third_of_a_frame(14);
        let (_cluster, mut executor) = holding_objects(&objects).await?;
        executor.start(STREAM, read_of_p(MAX_FRAME_FLOOR_U32)?);
        // The connection's credit, as its outbound queue keeps it: 4 to start, one spent by each batch that leaves, room
        // told while some is left.
        let mut credit = 4u32;
        let mut answer = ReadAnswer::default();
        let mut sent = 0u32;
        // Each pass takes one batch, granting a frame of Credit once credit is spent: fourteen objects bound the batches.
        for _ in 0..14 {
            let (_stream, frame) = next_of(&mut executor).await?;
            sent += 1;
            credit -= 1;
            if take(&mut answer, &frame)? {
                break;
            }
            if credit == 0 {
                assert!(sent >= 4, "4 frames leave on the Welcome's credit");
                assert!(
                    !gives_within(&mut executor, QUIET).await,
                    "none past the credit"
                );
                credit += 1;
            }
            executor.ready(STREAM);
        }
        assert!(sent > 4, "the answer takes more than 4 frames: {sent}");
        assert_eq!(answer.whole()?.len(), 14);
        Ok(())
    })
}

#[then("compute sends 4 frames, and one more for each frame of the client's Credit")]
fn four_frames_of_credit_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client cancels a read whose Scans are open at two partitions")]
fn a_cancel_of_two_scans(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let cluster = ApartCluster::start(2).await?;
        let lists: Vec<Held> = (1..=2).map(|partition| lists_of(partition, 2, 2)).collect();
        hold_all(&cluster, &lists, false)?;
        let forward = key_prefix(Keyspace::Forward, false, TENANT, Some(Graph::Default), None);
        for partition in 1..=2 {
            cluster
                .partition(partition)?
                .script(forward.clone(), Script::Silent, 0);
        }
        let core = cluster.core(times(Duration::from_secs(2)));
        let mut executor = ReadExecutor::new(Rc::clone(&core));
        executor.start(STREAM, read_of_p(MAX_FRAME_FLOOR_U32)?);
        for partition in 1..=2 {
            cluster.partition(partition)?.until_asked(&forward).await?;
        }
        executor.cancel(STREAM);
        assert!(
            !gives_within(&mut executor, QUIET).await,
            "no frame after the Cancel"
        );
        let (_links, holding) = links_holding_streams(&core).await;
        assert_eq!(holding, 0, "both Scans are cancelled");
        Ok(())
    })
}

#[then(
    "compute cancels both Scans in the turn it reads the Cancel, and no further Result frame is sent"
)]
fn a_cancel_of_two_scans_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a partition accepts a Scan of `deadline_ms` 50 and sends nothing, and `scan_stall_ms` is 100"
)]
fn a_silent_partition(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let cluster = ApartCluster::start(1).await?;
        let lists = lists_of(1, 1, 1);
        hold_all(&cluster, std::slice::from_ref(&lists), false)?;
        let forward = key_prefix(Keyspace::Forward, false, TENANT, Some(Graph::Default), None);
        let partition = cluster.partition(1)?;
        partition.script(forward.clone(), Script::Silent, 0);
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_millis(100))));
        executor.start(STREAM, read_of_p(MAX_FRAME_FLOOR_U32)?.with_deadline_ms(50));
        partition.until_asked(&forward).await?;
        let seen = now().await;
        let scans = partition.scans().len();
        let (_stream, frame) = next_of(&mut executor).await?;
        let waited = now().await.saturating_duration_since(seen);
        let mut answer = ReadAnswer::default();
        assert!(take(&mut answer, &frame)?);
        let (code, _detail, _message) = answer.refused.context("a refusal")?;
        assert_eq!(code, ErrorCode::Unavailable);
        // The deadline of 50 ms and twice the stall of 100: 250 ms from the Scan's sending, which came before it was seen
        // asked, so at least 150 ms from then.
        assert!(waited >= Duration::from_millis(150), "{waited:?}");
        assert_eq!(
            partition.scans().len(),
            scans,
            "no Scan of it is sent again"
        );
        Ok(())
    })
}

#[then(
    "compute closes the connection once 250 ms have passed, the read is answered code 17, and no Scan of it is sent again"
)]
fn a_silent_partition_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a partition sends a Scan's frames with gaps below twice `scan_stall_ms`, past the read's deadline"
)]
fn a_slow_partition(w: &mut Steps) -> TestResult {
    scenario(w, async {
        let cluster = ApartCluster::start(1).await?;
        let lists = lists_of(1, 3, 2);
        hold_all(&cluster, std::slice::from_ref(&lists), true)?;
        let forward = key_prefix(Keyspace::Forward, false, TENANT, Some(Graph::Default), None);
        let partition = cluster.partition(1)?;
        // Gaps of 150 ms: below twice the stall of 100, and past the read's deadline of 50.
        partition.script(forward, Script::Paced(150), 0);
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_millis(100))));
        let read = read_of_p(MAX_FRAME_FLOOR_U32)?.with_deadline_ms(50);
        let (answer, _sizes) = executed(&mut executor, read).await?;
        assert_eq!(
            answer.whole()?,
            rows_of_all(std::slice::from_ref(&lists)).as_slice()
        );
        assert_eq!(partition.connections(), 1, "the connection stayed open");
        Ok(())
    })
}

#[then("the connection stays open and the read reaches its `LAST`")]
fn a_slow_partition_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
