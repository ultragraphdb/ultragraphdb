/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A core's link to a partition carrying its reads beside its Bind streams (design D2): laws L4.1 to L4.4 of
//! `notes/laws.md`. The link alone is driven through the probe of `apart_link` against a partition double the test answers;
//! the reads, through a core over the links, against the played partitions. Each model is written from `PROTOCOL.md` §2.5
//! step 3, §2.6, §3's "Answered by", §11.3 and §9.1.

use std::time::Duration;

use compio::runtime::Runtime;
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_compute::{DoubleFrame, LinkProbe, ReadExecutor, RoleDouble, Told};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_protocol_wire::{
    Closed, ClosedPartition, Command, DEFAULT_INITIAL_CREDIT, ErrorCode, Held, MAX_FRAME_SIZE,
    MAX_FRAME_SIZE_U32,
};

use crate::apart::apart_cluster::{ApartCluster, times};
use crate::apart::apart_executor::{executed, pattern_of, read_frame, snapshot};
use crate::apart::apart_link::{
    PARTITION, Probed, bind, entries_on, frame_on, open_bind, probed, scan, scan_from, taken, told,
};
use crate::apart::apart_partition::{EPOCH, VERSION};
use crate::apart::apart_reads::{P, hold_lists, rows_listed};

/// How long a law waits to see that nothing more is told: the link tells what it tells within a turn or two of the frame.
const SETTLE: Duration = Duration::from_millis(30);

/// Runs `work` on a runtime of its own; `None` where the runtime cannot be built or the work fails or passes 20 s.
fn on_runtime<T>(work: impl std::future::Future<Output = Option<T>>) -> Option<T> {
    Runtime::new().ok()?.block_on(async {
        compio::time::timeout(Duration::from_secs(20), work)
            .await
            .ok()
            .flatten()
    })
}

/// What a stream on the modelled link was opened by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bind,
    Scan,
    Closed,
}

/// One step of L4.1's drawn sequence.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// A request opens a stream of this kind.
    Open(Kind),
    /// The partition answers the open stream drawn, by its request's answer, or by a command none of its requests is answered
    /// by.
    Reply { at: usize, answers: bool },
    /// A Write on the open Bind stream drawn fails to be written.
    FailWrite { at: usize },
    /// The partition closes the connection.
    PeerClose,
    /// A Scan whose frame cannot be built.
    Unbuildable,
}

/// Draws one step.
fn draw_step(tc: &TestCase) -> Step {
    let kind = tc.draw(gs::integers::<u8>().min_value(0).max_value(6));
    let at = tc.draw(gs::integers::<usize>().min_value(0).max_value(7));
    match kind {
        0 => Step::Open(Kind::Bind),
        1 => Step::Open(Kind::Scan),
        2 => Step::Open(Kind::Closed),
        3 => Step::Reply {
            at,
            answers: tc.draw(gs::booleans()),
        },
        4 => Step::FailWrite { at },
        5 => Step::PeerClose,
        _ => Step::Unbuildable,
    }
}

/// The model of one link: its open streams, their kinds and the requests the partition took for them, whether it is closed,
/// and the connections the partition took.
#[derive(Debug, Default)]
struct Model {
    open: Vec<(u64, Kind, DoubleFrame)>,
    closed: bool,
    dialled: usize,
}

/// Whether the link the probe holds was told, within [`SETTLE`], exactly `expected`, in any order across streams.
async fn told_exactly(probe: &mut LinkProbe, mut expected: Vec<(u64, Told)>) -> bool {
    let mut seen = Vec::new();
    for _ in 0..expected.len() {
        match told(probe).await {
            Ok(one) => seen.push(one),
            Err(_) => return false,
        }
    }
    let quiet = compio::time::timeout(SETTLE, probe.next_told())
        .await
        .is_err();
    seen.sort_by_key(|(stream, _)| *stream);
    expected.sort_by_key(|(stream, _)| *stream);
    quiet && seen == expected
}

/// Every open stream of `model` told the close, once each, and the model's link closed.
fn closing(model: &mut Model) -> Vec<(u64, Told)> {
    model.closed = true;
    model
        .open
        .drain(..)
        .map(|(stream, _, _)| (stream, Told::Closed))
        .collect()
}

/// Runs `step` on the probed link and its model: whether what the link told and holds is what the model says.
async fn run_step(probed: &mut Probed, model: &mut Model, step: Step) -> Option<bool> {
    let Probed { double, probe, .. } = probed;
    let expected = match step {
        Step::Open(kind) => {
            dials(model);
            let stream = match kind {
                Kind::Bind => open_bind(probe).ok()?,
                Kind::Scan => probe.open_read(&scan()).ok()?,
                Kind::Closed => probe.open_read(&Closed::request(&[PARTITION])).ok()?,
            };
            let command = match kind {
                Kind::Bind => Command::Bind,
                Kind::Scan => Command::Scan,
                Kind::Closed => Command::Closed,
            };
            let seen = double.count(command);
            let request = wait_for(double, command, seen).await?;
            model.open.push((stream, kind, request));
            Vec::new()
        }
        Step::Reply { at, answers } => {
            if model.open.is_empty() {
                return Some(true);
            }
            let (stream, kind, request) = model.open.get(at % model.open.len())?.clone();
            let (frame, told_as, ends) = reply(kind, stream, answers)?;
            request.answer_with(frame);
            if answers {
                if ends {
                    model.open.retain(|(open, _, _)| *open != stream);
                }
                vec![(stream, told_as)]
            } else {
                closing(model)
            }
        }
        Step::FailWrite { at } => {
            let binds: Vec<u64> = model
                .open
                .iter()
                .filter(|(_, kind, _)| *kind == Kind::Bind)
                .map(|(stream, _, _)| *stream)
                .collect();
            let Some(stream) = binds.get(at % binds.len().max(1)).copied() else {
                return Some(true);
            };
            probe.send_unwritable(stream);
            closing(model)
        }
        Step::PeerClose => {
            if model.dialled == 0 || model.closed {
                return Some(true);
            }
            double.drop_connections();
            closing(model)
        }
        Step::Unbuildable => {
            dials(model);
            let huge = vec![b'k'; MAX_FRAME_SIZE];
            let refused = probe.open_read(&scan_from(&huge)).err()?;
            if refused.code() != ErrorCode::Protocol {
                return Some(false);
            }
            Vec::new()
        }
    };
    let told_right = told_exactly(probe, expected).await;
    let held_right = model.closed || probe.streams_open() == model.open.len();
    Some(told_right && held_right && probe.is_closed() == model.closed)
}

/// A request is about to take the link: the partition takes a connection when the link it would go on has closed, or none was
/// dialled yet.
fn dials(model: &mut Model) {
    if model.closed || model.dialled == 0 {
        model.dialled += 1;
        model.closed = false;
    }
}

/// The partition's reply on `stream`, opened by `kind`: its request's answer when `answers`, else a command no request of
/// the stream is answered by (`PROTOCOL.md` §3, "Answered by"); what the stream is told of it, and whether it ends the stream.
fn reply(kind: Kind, stream: u64, answers: bool) -> Option<(Vec<u8>, Told, bool)> {
    let frame = match (kind, answers) {
        (Kind::Bind, true) => frame_on(stream, &bind().with_read_version(VERSION), false).ok()?,
        (Kind::Scan, true) => entries_on(stream, true).ok()?,
        (Kind::Closed, true) => frame_on(
            stream,
            &Closed::new(vec![ClosedPartition::new(PARTITION, EPOCH)]),
            true,
        )
        .ok()?,
        (Kind::Bind, false) => entries_on(stream, false).ok()?,
        (Kind::Scan | Kind::Closed, false) => frame_on(stream, &Held, false).ok()?,
    };
    let told_as = Told::Frame(match kind {
        Kind::Bind => Command::Bind,
        Kind::Scan => Command::Entries,
        Kind::Closed => Command::Closed,
    });
    Some((frame, told_as, kind != Kind::Bind))
}

/// Waits until `double` took more than `seen` requests of `command`: the newest.
async fn wait_for(double: &RoleDouble, command: Command, seen: usize) -> Option<DoubleFrame> {
    let waited = async {
        // Each look waits a millisecond; the timeout bounds them.
        loop {
            let frames = double.frames(command);
            if frames.len() > seen {
                return frames.into_iter().last();
            }
            compio::time::sleep(Duration::from_millis(1)).await;
        }
    };
    compio::time::timeout(Duration::from_secs(5), waited)
        .await
        .ok()
        .flatten()
}

/// L4.1: over a drawn sequence of Binds, Scans and Closed opened, the partition's replies, each its request's answer or a drawn
/// other, failed writes, the peer's closes and Scans that cannot be built, the link takes a reply exactly when a request of its
/// stream is answered by its command; any other frame closes it; a request that could not be sent registers no stream; once
/// closed, every open stream is told so exactly once, and the next request dials again.
#[hegel::test(test_cases = 10)]
fn test_the_link_agrees_with_its_model(tc: TestCase) {
    let count = tc.draw(gs::integers::<usize>().min_value(1).max_value(10));
    let steps: Vec<Step> = (0..count).map(|_| draw_step(&tc)).collect();
    let agreed = on_runtime(async {
        let mut probed = probed(false).await.ok()?;
        let mut model = Model::default();
        for step in &steps {
            if !run_step(&mut probed, &mut model, *step).await? {
                return Some(false);
            }
        }
        Some(probed.double.accepted() == model.dialled)
    });
    assert_eq!(agreed, Some(true), "{steps:?}");
}

/// L4.2: a Scan's stream granted `grants` frames beyond the partition's `initial_credit`, the tap letting the partition send
/// past it, and `frames` Entries frames sent: the link keeps every frame within the credit and stays open while they are; the
/// first frame past it closes the link, and the other read it carried is told the close.
#[hegel::test(test_cases = 10)]
fn test_frames_past_the_credit_close_the_link(tc: TestCase) {
    let grants = tc.draw(gs::integers::<u32>().min_value(0).max_value(3));
    let credit = DEFAULT_INITIAL_CREDIT + grants;
    let frames = tc.draw(gs::integers::<u32>().min_value(1).max_value(credit + 2));
    let past = frames > credit;
    let agreed = on_runtime(async {
        let Probed {
            double,
            tap,
            mut probe,
        } = probed(true).await.ok()?;
        tap?.overgrant(frames.saturating_sub(credit));
        let other = probe.open_read(&scan()).ok()?;
        let spent = probe.open_read(&scan()).ok()?;
        (0..grants).for_each(|_| probe.grant(spent));
        let request = taken(&double, Command::Scan, spent).await.ok()?;
        for _ in 0..frames {
            request.answer_with(entries_on(spent, false).ok()?);
        }
        let mut expected: Vec<(u64, Told)> = (0..frames.min(credit))
            .map(|_| (spent, Told::Frame(Command::Entries)))
            .collect();
        if past {
            expected.push((spent, Told::Closed));
            expected.push((other, Told::Closed));
        }
        let told_right = told_exactly(&mut probe, expected).await;
        Some(told_right && probe.is_closed() == past)
    });
    assert_eq!(
        agreed,
        Some(true),
        "{frames} frames for a credit of {credit}"
    );
}

/// How a frame on a stream the link holds no reader for is drawn.
#[derive(Debug, Clone, Copy)]
enum Stray {
    /// Whole and valid, on a stream that ended below the next.
    Late,
    /// On a stream that ended, its chunk's checksum broken (`PROTOCOL.md` §2.5, steps 11 and 12).
    Broken,
    /// On a stream at or above the next the link will open, this far past it.
    Ahead(u64),
}

/// L4.3: a frame on a stream whose reader is gone, below the next stream the link will open, is dropped and the link kept when
/// it is whole and valid, and refused when its checksum fails; one at or above that number closes the link whatever it holds.
#[hegel::test(test_cases = 10)]
fn test_a_frame_without_a_reader_is_checked_whole(tc: TestCase) {
    let gone = tc.draw(gs::integers::<usize>().min_value(1).max_value(3));
    let pick = tc.draw(gs::integers::<usize>().min_value(0).max_value(2));
    let stray = match tc.draw(gs::integers::<u8>().min_value(0).max_value(2)) {
        0 => Stray::Late,
        1 => Stray::Broken,
        _ => Stray::Ahead(tc.draw(gs::integers::<u64>().min_value(0).max_value(3))),
    };
    let closes = !matches!(stray, Stray::Late);
    let agreed = on_runtime(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await.ok()?;
        let mut ended = Vec::new();
        for _ in 0..gone {
            ended.push(probe.open_read(&scan()).ok()?);
        }
        let carrier = probe.open_read(&scan()).ok()?;
        let request = taken(&double, Command::Scan, carrier).await.ok()?;
        ended.iter().for_each(|stream| probe.cancel(*stream));
        let target = *ended.get(pick % ended.len())?;
        let frame = match stray {
            Stray::Late => entries_on(target, false).ok()?,
            Stray::Broken => {
                let mut broken = entries_on(target, false).ok()?;
                let last = broken.last_mut()?;
                *last ^= 0xFF;
                broken
            }
            Stray::Ahead(past) => {
                entries_on(probe.next_stream()?.saturating_add(past), false).ok()?
            }
        };
        request.answer_with(frame);
        let expected = if closes {
            vec![(carrier, Told::Closed)]
        } else {
            Vec::new()
        };
        let told_right = told_exactly(&mut probe, expected).await;
        Some(told_right && probe.is_closed() == closes)
    });
    assert_eq!(agreed, Some(true), "{stray:?}");
}

/// L4.4: 2 to 4 played partitions bound to their ports in a drawn order, each holding a list of its own: a read binding `<p>`
/// alone returns L5.1's model, partition by partition in ascending order of partition, whatever the order they bound in, and
/// asks each partition by its number.
#[hegel::test(test_cases = 10)]
fn test_a_read_reaches_its_partitions_whatever_the_order_bound(tc: TestCase) {
    let partitions = tc.draw(gs::integers::<u32>().min_value(2).max_value(4));
    let mut order: Vec<u32> = (1..=partitions).collect();
    // A drawn permutation: each place takes one of the numbers left.
    let mut drawn = Vec::new();
    while !order.is_empty() {
        let at = tc.draw(
            gs::integers::<usize>()
                .min_value(0)
                .max_value(order.len() - 1),
        );
        drawn.push(order.remove(at));
    }
    let model: Vec<Vec<Option<String>>> = (1..=partitions)
        .flat_map(|partition| {
            let subject = format!("http://ex.org/s{partition}");
            let object = format!("http://ex.org/o{partition}");
            rows_listed(&[(subject.as_str(), &[object.as_str()])])
        })
        .collect();
    let answered = on_runtime(async {
        let cluster = ApartCluster::start_in(&drawn, false).await.ok()?;
        for partition in 1..=partitions {
            let subject = format!("http://ex.org/s{partition}");
            let object = format!("http://ex.org/o{partition}");
            let lists: &[(&str, &[&str])] = &[(subject.as_str(), &[object.as_str()])];
            hold_lists(
                cluster.partition(partition).ok()?,
                (Graph::Default, P),
                lists,
                u64::from(partition) * 100,
            )
            .ok()?;
        }
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(2))));
        let pattern = pattern_of(&read_frame((false, None, Some(P)), 0, Vec::new()).ok()?).ok()?;
        let (answer, _sizes) = executed(&mut executor, snapshot(pattern, MAX_FRAME_SIZE_U32))
            .await
            .ok()?;
        let asked = (1..=partitions).all(|partition| {
            cluster
                .partition(partition)
                .is_ok_and(|played| !played.scans().is_empty())
        });
        Some((answer.rows, answer.refused, asked))
    });
    assert_eq!(
        answered,
        Some((model, None, true)),
        "bound in the order {drawn:?}"
    );
}
