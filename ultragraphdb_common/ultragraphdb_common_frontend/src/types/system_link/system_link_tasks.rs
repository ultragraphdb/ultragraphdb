/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A link's connection: dialled and dialled again, its sending task running the turns, its reading task taking the answers
//! and the closes.

use std::future::poll_fn;
use std::task::Poll;
use std::time::Duration;

use compio::runtime::spawn;
use compio::time::timeout;
use ultragraphdb_common_role::{RoleClient, RoleCluster, RoleReader, RoleWriter};
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CheckView, Command, Credit, EpochCloseView, ErrorReplyView, FLAG_LAST,
    Header, Subscribe,
};

use super::{LinkState, SystemLink, unavailable};
use crate::types::client_session::now;
use crate::types::system_turns::SystemTurns;

/// How long a dial that failed waits before the next.
const REDIAL: Duration = Duration::from_millis(100);

/// Dials the system group at `address` and keeps the link's connection, dialling again each time it closes, for the core's
/// lifetime. Spawned detached: a future dropped at the runtime's end leaves the connection to it.
pub(super) fn spawn_connection(
    link: SystemLink,
    address: Address,
    cluster: RoleCluster,
    dial_ms: u32,
) {
    spawn(async move {
        let bound = Duration::from_millis(u64::from(dial_ms));
        // An event loop: it dials for as long as the core runs, which ends with the service, or until the link is stopped (Ultra
        // rule 2).
        while !link.state.borrow().stopped {
            let dialled = timeout(
                bound,
                RoleClient::dial(&address, cluster.id(), "frontend", cluster.secret().password()),
            )
            .await;
            match dialled {
                Ok(Ok(client)) => run(&link, client).await,
                Ok(Err(error)) => {
                    tracing::debug!(address = %address, %error, "the system group could not be dialled");
                }
                Err(_) => tracing::warn!(
                    "the system group at {address} did not finish its connect, Hello and SCRAM exchange within {dial_ms} ms"
                ),
            }
            compio::time::sleep(REDIAL).await;
        }
    })
    .detach();
}

/// Runs one connection until it closes: subscribed with `latest` first, its sending task beside its reading.
async fn run(link: &SystemLink, client: RoleClient) {
    let (reader, mut writer, welcome) = client.split();
    if link.state.borrow().stopped {
        writer.shutdown().await;
        return;
    }
    let reopened = {
        let mut state = link.state.borrow_mut();
        state.open = true;
        state.lost_since = None;
        state.next_stream = 1;
        state.max_frame = welcome.max_frame();
        state.checks_max = SystemTurns::checks_max(welcome.max_frame());
        state.ended_max = SystemTurns::ended_max(welcome.max_frame());
        let reopened = state.subscription.is_some() || state.view.latest_closed() != 0;
        // The sending task starts below and sends what waits: no task is waiting to be woken yet.
        let _unstarted = subscribe(&mut state);
        reopened
    };
    if reopened {
        link.state.borrow_mut().view.reopened();
    }
    link.notify();
    let sending = spawn(send(link.clone(), writer));
    // The read ends when the connection does, or once the link is stopped and its sending side shut.
    let reading = read(link, reader);
    let stopping = poll_fn(|cx| {
        let mut state = link.state.borrow_mut();
        if state.stopped {
            return Poll::Ready(());
        }
        state.watchers.push(cx.waker().clone());
        Poll::Pending
    });
    let _first = futures::future::select(std::pin::pin!(reading), std::pin::pin!(stopping)).await;
    closed(link);
    drop(sending.await);
}

/// Queues a Subscribe with `latest` on a stream of its own: the sending task's waker, for after the borrow.
fn subscribe(state: &mut LinkState) -> Option<std::task::Waker> {
    let stream = state.stream();
    let header = Header::new(state.cluster, Command::Subscribe).with_stream(stream);
    let frame = Subscribe::at_latest().frame(header).ok()?;
    state.subscription = Some(stream);
    state.close_parts.clear();
    state.queue(frame)
}

/// The sending task: each run sends what waits, then a turn's Ended frame and Check frame, until the connection closes.
/// A future dropped at the runtime's end may cut the frame being written.
async fn send(link: SystemLink, mut writer: RoleWriter) {
    // An event loop: the connection's close ends it (Ultra rule 2).
    while let Some(frame) = next_frame(&link).await {
        if let Err(error) = writer.send(frame).await {
            tracing::debug!(%error, "the system group's connection failed a write");
            link.state.borrow_mut().open = false;
            break;
        }
    }
    writer.shutdown().await;
}

/// The next frame to send: one waiting, or a turn's frames once nothing waits; `None` once the connection closed.
async fn next_frame(link: &SystemLink) -> Option<Vec<u8>> {
    poll_fn(|cx| {
        let mut state = link.state.borrow_mut();
        if !state.open {
            return Poll::Ready(None);
        }
        if state.outgoing.is_empty() && !state.turns.is_empty() {
            turn(&mut state);
        }
        if let Some(frame) = state.outgoing.pop_front() {
            return Poll::Ready(Some(frame));
        }
        state.sender = Some(cx.waker().clone());
        Poll::Pending
    })
    .await
}

/// One turn (`PROTOCOL.md` §11.5, *Turns*): the Ended frame, then the Check frame, of what the core's requests queued
/// since the last; the Check frame's checks wait for its reply, timed from now.
fn turn(state: &mut LinkState) {
    let (checks_max, ended_max) = (state.checks_max, state.ended_max);
    let turn = state.turns.take(checks_max, ended_max);
    let ended_stream = state.stream();
    if let Some(frame) =
        turn.ended_frame(Header::new(state.cluster, Command::Ended).with_stream(ended_stream))
    {
        state.outgoing.push_back(frame);
    }
    if turn.checks().is_empty() {
        return;
    }
    let check_stream = state.stream();
    match turn.check_frame(Header::new(state.cluster, Command::Check).with_stream(check_stream)) {
        Some(frame) => {
            state.outgoing.push_back(frame);
            let _none = state
                .checks_out
                .insert(check_stream, (turn.checks().to_vec(), now()));
        }
        None => {
            for (ticket, _) in turn.checks() {
                if let Some(waiting) = state.waiting.remove(ticket) {
                    drop(waiting.send(Err(unavailable("a Check frame could not be built"))));
                }
            }
        }
    }
}

/// The reading task: hands each frame to what waits on its stream, until the connection ends.
async fn read(link: &SystemLink, mut reader: RoleReader) {
    // An event loop: the connection's end ends it (Ultra rule 2).
    loop {
        match reader.next_frame().await {
            Ok(frame) => deliver(link, &frame),
            Err(error) => {
                tracing::debug!(%error, "the system group's connection ended");
                return;
            }
        }
        if !link.state.borrow().open {
            return;
        }
    }
}

/// `frame` to what waits on its stream: the subscription's closes to the view, a Check frame's outcomes to its checks, an
/// ask's frames to its asker. A frame on a stream nothing waits on is late, and dropped.
fn deliver(link: &SystemLink, frame: &ReceivedFrame) {
    let Ok(view) = frame.view() else {
        return;
    };
    let stream = view.header().stream();
    let last = view.header().flags() & FLAG_LAST != 0;
    let subscription = link.state.borrow().subscription;
    if Some(stream) == subscription {
        return delivered_close(link, &view);
    }
    let checks = link.state.borrow_mut().checks_out.remove(&stream);
    if let Some((checks, sent)) = checks {
        return delivered_checks(link, &view, &checks, sent);
    }
    let mut state = link.state.borrow_mut();
    let Some(ask) = state.asks.get_mut(&stream) else {
        return;
    };
    if view.command() == Command::Error {
        let refusal = ErrorReplyView::from_frame(&view)
            .and_then(|error| error.to_body())
            .unwrap_or_else(|_| unavailable("an Error that does not read"));
        if let Some(ask) = state.asks.remove(&stream) {
            drop(ask.reply.send(Err(refusal)));
        }
        return;
    }
    ask.frames.push(frame.bytes().to_vec());
    if last && let Some(ask) = state.asks.remove(&stream) {
        drop(ask.reply.send(Ok(ask.frames)));
        return;
    }
    // Catalog and Defaults frames spend credit as Result frames do (`PROTOCOL.md` §13.7, §13.28): each frame read grants the
    // next.
    if !last && (view.command() == Command::Catalog || view.command() == Command::Defaults) {
        let header = Header::new(state.cluster, Command::Credit).with_stream(stream);
        if let Ok(credit) = Credit::new(1).frame(header) {
            let sender = state.queue(credit);
            drop(state);
            if let Some(sender) = sender {
                sender.wake();
            }
        }
    }
}

/// A frame of the subscription: a close, applied once whole, or the subscription's end, which opens it again.
fn delivered_close(link: &SystemLink, view: &ultragraphdb_protocol_wire::FrameView<'_>) {
    if view.command() != Command::EpochClose {
        // Ended as no longer held: opened again with `latest`, skipping closes (`PROTOCOL.md` §13.3).
        let mut state = link.state.borrow_mut();
        let sender = subscribe(&mut state);
        state.view.reopened();
        drop(state);
        if let Some(sender) = sender {
            sender.wake();
        }
        return;
    }
    let Ok(close) = EpochCloseView::from_frame(view) else {
        return;
    };
    let announced: Vec<(u64, u64)> = (0..close.announcement_count())
        .filter_map(|at| close.announcement(at))
        .map(|announcement| (announcement.tenant(), announcement.version()))
        .collect();
    {
        let mut state = link.state.borrow_mut();
        state
            .close_parts
            .push((close.epoch(), close.placement(), announced));
        if close.more() {
            return;
        }
        let parts = std::mem::take(&mut state.close_parts);
        let epoch = parts.first().map_or(close.epoch(), |part| part.0);
        let placement = parts.iter().map(|part| part.1).max().unwrap_or(0);
        let announced: Vec<(u64, u64)> = parts.into_iter().flat_map(|part| part.2).collect();
        if placement != 0 {
            state.placement_asked = placement;
        }
        let _applied = state.view.close(epoch, placement, &announced);
    }
    link.notify();
    link.refresh();
}

/// A Check frame's reply: each check's outcome to its waiter, checked against what was sent, and its round trip counted.
fn delivered_checks(
    link: &SystemLink,
    view: &ultragraphdb_protocol_wire::FrameView<'_>,
    checks: &[(u64, ultragraphdb_protocol_wire::SessionCheck)],
    sent: std::time::Instant,
) {
    let answered = now();
    let mut state = link.state.borrow_mut();
    state.round_trips.record(sent, answered);
    let sent_checks: Vec<_> = checks.iter().map(|(_, check)| *check).collect();
    let outcomes = if view.command() == Command::Check {
        CheckView::from_frame(view)
            .and_then(|reply| reply.check_reply(&sent_checks).map(|()| reply))
            .map(|reply| {
                (0..reply.len())
                    .filter_map(|at| reply.get(at))
                    .map(|check| check.outcome())
                    .collect::<Vec<_>>()
            })
            .map_err(|rule| ultragraphdb_protocol_wire::ErrorReply::protocol(&rule))
    } else {
        Err(ErrorReplyView::from_frame(view)
            .and_then(|error| error.to_body())
            .unwrap_or_else(|_| unavailable("a Check refused")))
    };
    for (at, (ticket, _)) in checks.iter().enumerate() {
        let Some(waiting) = state.waiting.remove(ticket) else {
            continue;
        };
        let outcome = match &outcomes {
            Ok(outcomes) => outcomes
                .get(at)
                .copied()
                .ok_or_else(|| unavailable("a Check reply of fewer checks")),
            Err(refusal) => Err(refusal.clone()),
        };
        drop(waiting.send(outcome));
    }
}

/// The connection closed: what waits on it is refused with code 17, the checks queued for it included, and the Endeds
/// queued stay for the next.
fn closed(link: &SystemLink) {
    {
        let mut state = link.state.borrow_mut();
        state.open = false;
        state.lost_since = Some(now());
        // The frontend is unready from the connection's loss until the group answers again (role-binaries, "Readiness,
        // health and shutdown").
        state.readiness.set(false);
        state.subscription = None;
        state.outgoing.clear();
        state.checks_out.clear();
        let tickets: Vec<u64> = state.waiting.keys().copied().collect();
        for ticket in tickets {
            let _queued = state.turns.withdraw(ticket);
            if let Some(waiting) = state.waiting.remove(&ticket) {
                drop(waiting.send(Err(unavailable(
                    "the connection to the system group closed under the check",
                ))));
            }
        }
        for (_, ask) in state.asks.drain() {
            drop(ask.reply.send(Err(unavailable(
                "the connection to the system group closed under the request",
            ))));
        }
        if let Some(sender) = state.sender.take() {
            sender.wake();
        }
    }
    link.notify();
}
