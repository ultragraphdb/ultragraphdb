/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The executor's laws, L5.1, L5.2, L5.4 to L5.7, L5.9, L5.10 and L5.14 of `notes/laws.md`, over played partitions: the
//! model of a read's answer is `PROTOCOL.md` §9.1's, the rows of the partitions asked partition by partition in ascending
//! order of partition, within a partition in the order its entries give them.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_compute::ReadExecutor;
use ultragraphdb_common_role::OpenRequests;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_protocol_wire::{
    MAX_FRAME_FLOOR, MAX_FRAME_FLOOR_U32, MAX_FRAME_SIZE_U32, RequestTerm, ResumeKey,
    ResumeKeyView, ResumeRow, TermTable,
};

use crate::apart::apart_cluster::{ApartCluster, times};
use crate::apart::apart_executor::{
    STREAM, bound, executed, gives_within, next_of, pattern_of, read_frame, snapshot, take,
};
use crate::apart::apart_reads::{P, ReadAnswer, answer_on, hold_lists, read_of, rows_listed};

/// Rows a model holds: each its subject, its object and an absent reifier.
type Rows = Vec<Vec<Option<String>>>;

/// What one partition holds: each subject's IRI and its objects' IRIs, in the order its keys sort.
type Held = Vec<(String, Vec<String>)>;

/// Draws one partition's lists: up to `subjects` subjects of `<p>`, each with one to three objects, some objects long enough
/// that a client's frame of the floor holds few of them.
fn draw_held(tc: &TestCase, partition: u32, subjects: usize) -> Held {
    let count = tc.draw(gs::integers::<usize>().min_value(0).max_value(subjects));
    (0..count)
        .map(|subject| {
            let objects = tc.draw(gs::integers::<usize>().min_value(1).max_value(3));
            let objects = (0..objects)
                .map(|object| {
                    let long = tc.draw(gs::booleans());
                    let pad = if long {
                        "x".repeat(MAX_FRAME_FLOOR / 5)
                    } else {
                        String::new()
                    };
                    format!("http://ex.org/{partition}/{subject}/{object}/{pad}")
                })
                .collect();
            (format!("http://ex.org/{partition}/s{subject}"), objects)
        })
        .collect()
}

/// `held`'s rows in the order its keys sort, which [`hold_lists`] makes the order listed.
fn rows_of_held(held: &Held) -> Rows {
    let lists: Vec<(&str, Vec<&str>)> = held
        .iter()
        .map(|(subject, objects)| {
            (
                subject.as_str(),
                objects.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &[&str])> = lists
        .iter()
        .map(|(subject, objects)| (*subject, objects.as_slice()))
        .collect();
    rows_listed(&borrowed)
}

/// Has `cluster`'s partition `partition` hold `held`, its IDs drawn from `first`, framed per key when `per_key`.
fn hold(
    cluster: &ApartCluster,
    partition: u32,
    held: &Held,
    first: u64,
    per_key: bool,
) -> Option<()> {
    let lists: Vec<(&str, Vec<&str>)> = held
        .iter()
        .map(|(subject, objects)| {
            (
                subject.as_str(),
                objects.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &[&str])> = lists
        .iter()
        .map(|(subject, objects)| (*subject, objects.as_slice()))
        .collect();
    let played = cluster.partition(partition).ok()?;
    hold_lists(played, (Graph::Default, P), &borrowed, first).ok()?;
    if per_key {
        played.frame_per_key();
    }
    Some(())
}

/// Runs `work` on a runtime of its own; `None` where the runtime cannot be built or the work fails.
fn on_runtime<T>(work: impl std::future::Future<Output = Option<T>>) -> Option<T> {
    Runtime::new().ok()?.block_on(async {
        compio::time::timeout(Duration::from_secs(20), work)
            .await
            .ok()
            .flatten()
    })
}

/// L5.1 and L5.2: over 1 to 3 partitions, each holding drawn lists under IDs drawn so that keys sort across partitions in any
/// order, framed per key or not, and a client's `max_frame` of the floor or the most, a read binding `<p>` alone returns the
/// model's rows in the model's order, each once; every batch is within `max_frame`, and a stream holds at least one batch.
#[hegel::test(test_cases = 15)]
fn test_a_read_answers_partition_by_partition(tc: TestCase) {
    let partitions = tc.draw(gs::integers::<u32>().min_value(1).max_value(3));
    let helds: Vec<(Held, u64, bool)> = (1..=partitions)
        .map(|partition| {
            let held = draw_held(&tc, partition, 3);
            let first = tc.draw(gs::integers::<u64>().min_value(1).max_value(9)) * 100;
            (held, first, tc.draw(gs::booleans()))
        })
        .collect();
    let max_frame = if tc.draw(gs::booleans()) {
        MAX_FRAME_FLOOR_U32
    } else {
        MAX_FRAME_SIZE_U32
    };
    let model: Rows = helds
        .iter()
        .flat_map(|(held, _, _)| rows_of_held(held))
        .collect();
    let answered = on_runtime(async {
        let cluster = ApartCluster::start(partitions).await.ok()?;
        for (at, (held, first, per_key)) in helds.iter().enumerate() {
            hold(
                &cluster,
                u32::try_from(at).ok()? + 1,
                held,
                *first,
                *per_key,
            )?;
        }
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(2))));
        let pattern = pattern_of(&read_frame((false, None, Some(P)), 0, Vec::new()).ok()?).ok()?;
        executed(&mut executor, snapshot(pattern, max_frame))
            .await
            .ok()
    });
    assert!(answered.is_some(), "the read did not run");
    let Some((answer, sizes)) = answered else {
        return;
    };
    assert_eq!(answer.refused, None);
    assert_eq!(answer.rows, model);
    assert!(!sizes.is_empty());
    assert!(
        sizes.iter().all(|size| *size <= max_frame as usize),
        "{sizes:?}"
    );
}

/// L5.4 and L5.6: a read of drawn lists under a drawn `limit`, read page after page by each page's resume key, each page through
/// another executor of its own core, concatenates to the unpaged answer, no row repeated and none skipped.
#[hegel::test(test_cases = 15)]
fn test_pages_read_by_their_keys_concatenate_to_the_whole(tc: TestCase) {
    let held = draw_held(&tc, 1, 4);
    let model = rows_of_held(&held);
    let limit = tc.draw(gs::integers::<u32>().min_value(1).max_value(5));
    let pages = on_runtime(async {
        let cluster = ApartCluster::start(1).await.ok()?;
        hold(&cluster, 1, &held, 100, false)?;
        let mut rows = Rows::new();
        let mut resume = Vec::new();
        // Each pass reads one page of at least one row, or the empty last one: the model's rows bound it.
        for _ in 0..=model.len() {
            let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(2))));
            let frame = read_frame((false, None, Some(P)), limit, resume).ok()?;
            let (answer, _sizes) = executed(&mut executor, bound(pattern_of(&frame).ok()?))
                .await
                .ok()?;
            if answer.refused.is_some() {
                return None;
            }
            rows.extend(answer.rows);
            if answer.resume.is_empty() {
                return Some(rows);
            }
            resume = answer.resume;
        }
        None
    });
    assert_eq!(pages, Some(model));
}

/// L5.5: a resume key's fields read back as drawn; with one drawn byte changed it is refused.
#[hegel::test(test_cases = 100)]
fn test_a_resume_key_reads_back_its_fields(tc: TestCase) {
    let pattern = tc.draw(gs::integers::<u64>());
    let scope = tc.draw(gs::integers::<u64>());
    let partition = tc.draw(gs::integers::<u32>().min_value(1));
    let mut terms = TermTable::new();
    let subject = terms.iri(&format!("http://ex.org/{}", tc.draw(gs::integers::<u16>())));
    let predicate = terms.iri("http://ex.org/p");
    let lexical = tc.draw(gs::text().max_size(12));
    let Ok(object) = terms.intern(RequestTerm::Literal {
        lexical,
        datatype: None,
    }) else {
        return;
    };
    let ordering = tc.draw(gs::vecs(gs::integers::<u8>()).max_size(9));
    let row = ResumeRow::new(None, subject, predicate, object, None);
    let bytes = ResumeKey::new(pattern, scope, partition, terms, row)
        .with_ordering(ordering.clone())
        .encode();
    let read = ResumeKeyView::read(&bytes).ok();
    let fields = read.as_ref().map(|view| {
        (
            view.pattern(),
            view.scope(),
            view.partition(),
            view.ordering().to_vec(),
            view.row(),
        )
    });
    assert_eq!(fields, Some((pattern, scope, partition, ordering, row)));
    let at = tc.draw(
        gs::integers::<usize>()
            .min_value(0)
            .max_value(bytes.len().saturating_sub(1)),
    );
    let flip = tc.draw(gs::integers::<u8>().min_value(1));
    let mut changed = bytes.clone();
    if let Some(byte) = changed.get_mut(at) {
        *byte ^= flip;
    }
    assert!(
        ResumeKeyView::read(&changed).is_err(),
        "byte {at} changed by {flip}"
    );
}

/// L5.7: under room given at drawn points, the batches handed out are never more than the room given and the first batch,
/// and with room enough every row arrives.
#[hegel::test(test_cases = 12)]
fn test_no_batch_is_handed_out_past_its_room(tc: TestCase) {
    let held: Held = vec![(
        "http://ex.org/1/s".to_string(),
        (0..tc.draw(gs::integers::<usize>().min_value(1).max_value(7)))
            .map(|at| format!("http://ex.org/1/{at}/{}", "x".repeat(MAX_FRAME_FLOOR / 3)))
            .collect(),
    )];
    let model = rows_of_held(&held);
    let holds: Vec<bool> = (0..8).map(|_| tc.draw(gs::booleans())).collect();
    let answered = on_runtime(async {
        let cluster = ApartCluster::start(1).await.ok()?;
        hold(&cluster, 1, &held, 100, false)?;
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(2))));
        let frame = read_frame((false, None, Some(P)), 0, Vec::new()).ok()?;
        executor.start(
            STREAM,
            bound(pattern_of(&frame).ok()?).with_max_frame(MAX_FRAME_FLOOR_U32),
        );
        let mut answer = ReadAnswer::default();
        let (mut handed, mut rooms) = (0u32, 0u32);
        // Each pass takes one batch: the model's rows, a third of a frame each, bound the batches.
        for at in 0..=model.len() {
            let (_stream, frame) = next_of(&mut executor).await.ok()?;
            handed += 1;
            if handed > rooms + 1 {
                return None;
            }
            if take(&mut answer, &frame).ok()? {
                return Some(answer);
            }
            if holds.get(at).copied().unwrap_or(false)
                && gives_within(&mut executor, Duration::from_millis(15)).await
            {
                return None;
            }
            executor.ready(STREAM);
            rooms += 1;
        }
        None
    });
    assert_eq!(answered.map(|answer| answer.rows), Some(model));
}

/// L5.9 and L5.10: 1 to 3 reads under way on one core, each with every Entries frame ready: a task that runs whenever it is
/// let sees the core's step count move by at most one step of each read between two of its runs.
#[hegel::test(test_cases = 8)]
fn test_no_task_waits_more_than_one_step(tc: TestCase) {
    let reads = tc.draw(gs::integers::<usize>().min_value(1).max_value(3));
    let subjects = tc.draw(gs::integers::<usize>().min_value(1).max_value(6));
    let held: Held = (0..subjects)
        .map(|subject| {
            (
                format!("http://ex.org/1/s{subject}"),
                (0..3)
                    .map(|object| format!("http://ex.org/1/o{object}"))
                    .collect(),
            )
        })
        .collect();
    let widest = on_runtime(async {
        let cluster = ApartCluster::start(1).await.ok()?;
        hold(&cluster, 1, &held, 100, true)?;
        let core = cluster.core(times(Duration::from_secs(2)));
        let widest = Rc::new(Cell::new(0u64));
        let done = Rc::new(Cell::new(false));
        let (seen, stop, watched) = (Rc::clone(&widest), Rc::clone(&done), Rc::clone(&core));
        let probe = spawn(async move {
            let mut last = watched.steps();
            while !stop.get() {
                yield_now().await;
                let now = watched.steps();
                seen.set(seen.get().max(now.saturating_sub(last)));
                last = now;
            }
        });
        let mut runs = Vec::new();
        for _ in 0..reads {
            let core = Rc::clone(&core);
            runs.push(spawn(async move {
                let mut executor = ReadExecutor::new(core);
                let frame = read_frame((false, None, Some(P)), 0, Vec::new()).ok()?;
                executed(&mut executor, bound(pattern_of(&frame).ok()?))
                    .await
                    .ok()
            }));
        }
        for run in runs {
            run.await.ok()??;
        }
        done.set(true);
        drop(probe);
        Some(widest.get())
    });
    let reads = u64::try_from(reads).unwrap_or(u64::MAX);
    assert!(
        widest.is_some_and(|widest| widest <= reads),
        "{widest:?} of {reads} reads"
    );
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

/// L5.14: a read of drawn lists answers the same rows, batches and resume key through the session's seam in process and as a
/// ComputeRead through `ComputeSession`, over the same played partitions.
#[hegel::test(test_cases = 10)]
fn test_both_callers_get_one_answer(tc: TestCase) {
    let held = draw_held(&tc, 1, 3);
    let limit = tc.draw(gs::integers::<u32>().min_value(0).max_value(4));
    let both = on_runtime(async {
        let cluster = ApartCluster::start(1).await.ok()?;
        hold(&cluster, 1, &held, 100, false)?;
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(2))));
        let frame = read_frame((false, None, Some(P)), limit, Vec::new()).ok()?;
        let (in_process, _sizes) = executed(&mut executor, bound(pattern_of(&frame).ok()?))
            .await
            .ok()?;
        let mut client = cluster
            .compute(times(Duration::from_secs(2)), OpenRequests::default())
            .await
            .ok()?;
        let relayed = answer_on(
            &mut client,
            &read_of((false, None, Some(P)), limit, Vec::new()),
        )
        .await
        .ok()?;
        Some((in_process, relayed))
    });
    assert!(both.is_some(), "a read did not run");
    let Some((in_process, relayed)) = both else {
        return;
    };
    assert_eq!(in_process.rows, relayed.rows);
    assert_eq!(in_process.batches, relayed.batches);
    assert_eq!(in_process.resume, relayed.resume);
    assert_eq!(in_process.refused, relayed.refused);
}
