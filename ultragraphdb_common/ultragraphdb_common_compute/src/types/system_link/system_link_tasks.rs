/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A link's connection: dialled and dialled again, its sending task running the turns, its reading task taking the answers
//! and the closes.

use std::future::poll_fn;
use std::task::{Poll, Waker};
use std::time::{Duration, Instant};

use compio::runtime::spawn;
use compio::time::timeout;
use ultragraphdb_common_role::{RoleClient, RoleCluster, RoleReader, RoleWriter};
use ultragraphdb_common_service::RoleKind;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CheckView, Command, Credit, EpochCloseView, ErrorReply, ErrorReplyView,
    FLAG_LAST, FrameView, Header, SessionCheck, Subscribe,
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
        let name = RoleKind::Compute.to_string();
        // An event loop: it dials for as long as the core runs, which ends with the service, or until the link is stopped (Ultra
        // rule 2).
        while !link.state.borrow().stopped {
            let dialled = timeout(
                bound,
                RoleClient::dial(&address, cluster.id(), &name, cluster.secret().password()),
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

/// Runs one connection until it closes: subscribed with `latest` first, its sending task beside its reading. A future
/// dropped at the runtime's end leaves the link as it stood, and its sending task to the runtime.
async fn run(link: &SystemLink, client: RoleClient) {
    let (reader, mut writer, welcome) = client.split();
    if link.state.borrow().stopped {
        writer.shutdown().await;
        return;
    }
    let reopened = {
        let mut state = link.state.borrow_mut();
        state.open = true;
        state.connection = state.connection.wrapping_add(1);
        state.refused = false;
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
    // The read ends when the connection does, or once the link is stopped and its sending side shut, or an answer read on it
    // was refused.
    let reading = read(link, reader);
    let stopping = poll_fn(|context| {
        let mut state = link.state.borrow_mut();
        if state.stopped || state.refused {
            return Poll::Ready(());
        }
        state.watchers.push(context.waker().clone());
        Poll::Pending
    });
    // Whichever ends first ends the connection, and the other is dropped with it.
    let _first = futures::future::select(std::pin::pin!(reading), std::pin::pin!(stopping)).await;
    closed(link);
    // Awaited so the sending side is shut before the next dial; the task returns nothing to read.
    drop(sending.await);
}

/// Queues a Subscribe with `latest` on a stream of its own: the sending task's waker, for after the borrow.
fn subscribe(state: &mut LinkState) -> Option<Waker> {
    let stream = state.stream();
    let header = Header::new(state.cluster, Command::Subscribe).with_stream(stream);
    // A Subscribe that cannot be built queues nothing, and the link holds no subscription on this connection.
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

/// The next frame to send: one waiting, or a turn's frames once nothing waits; `None` once the connection closed. A
/// dropped future leaves what waits queued and its waker registered, which the next frame wakes for nothing.
async fn next_frame(link: &SystemLink) -> Option<Vec<u8>> {
    poll_fn(|context| {
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
        state.sender = Some(context.waker().clone());
        Poll::Pending
    })
    .await
}

/// One turn (`PROTOCOL.md` §11.5, *Turns*): the Ended frame, then the Check frame, of what the core's requests queued
/// since the last; the Check frame's checks wait for its reply, timed from now.
fn turn(state: &mut LinkState) {
    debug_assert!(
        state.outgoing.is_empty(),
        "a turn runs once nothing waits to leave"
    );
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
                    // A waiter gone has no one to tell.
                    drop(waiting.send(Err(unavailable("a Check frame could not be built"))));
                }
            }
        }
    }
}

/// The reading task: hands each frame to what waits on its stream, until the connection ends, or until the link refuses a
/// frame, which ends it as the side that dialled refusing a frame does (`PROTOCOL.md` §2.6, §13): the frames after a refused
/// one answer nothing on this connection, so what waits on it is refused and the link dials again. A future dropped at
/// the runtime's end drops the reader with a frame read in part.
async fn read(link: &SystemLink, mut reader: RoleReader) {
    // An event loop: the connection's end ends it (Ultra rule 2).
    loop {
        match reader.next_frame().await {
            Ok(frame) => {
                let delivered = frame
                    .view()
                    .map_err(|rule| rule.to_string())
                    .and_then(|view| deliver(link, &frame, &view));
                if let Err(refused) = delivered {
                    tracing::warn!(%refused, "the system group sent a frame the link refuses; closing the connection");
                    return;
                }
            }
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

/// `frame`, validated as `view`, to what waits on its stream: the subscription's closes to the view, a Check frame's outcomes
/// to its checks, an ask's frames to its asker. A frame on a stream nothing waits on, below the next the link opens, is late,
/// and dropped (`PROTOCOL.md` §2.5, step 3).
///
/// # Errors
///
/// Why the link refuses it, which closes the connection: a frame on stream 0, the group's connection error or a command not
/// served there, or on a stream not yet opened; a command its request is not answered by; a body breaking its command's
/// rules (`PROTOCOL.md` §2.5, §2.6, §13).
fn deliver(link: &SystemLink, frame: &ReceivedFrame, view: &FrameView<'_>) -> Result<(), String> {
    let stream = view.header().stream();
    let last = view.header().flags() & FLAG_LAST != 0;
    let (subscription, next) = {
        let state = link.state.borrow();
        (state.subscription, state.next_stream)
    };
    if stream == 0 || stream >= next {
        return Err(format!(
            "a frame of command {} on stream {stream}, which it holds no request on",
            view.command().number()
        ));
    }
    if Some(stream) == subscription {
        return delivered_close(link, view);
    }
    let checks = link.state.borrow_mut().checks_out.remove(&stream);
    if let Some((checks, sent)) = checks {
        return delivered_checks(link, view, &checks, sent);
    }
    let mut state = link.state.borrow_mut();
    let Some(asked_by) = state.asks.get(&stream).map(|ask| ask.command) else {
        return Ok(());
    };
    if view
        .command()
        .answering(asked_by)
        .map_err(|rule| rule.to_string())?
        == Command::Error
    {
        let refusal = error_of(view)?;
        if let Some(ask) = state.asks.remove(&stream) {
            // An asker gone has no one to tell.
            drop(ask.reply.send(Err(refusal)));
        }
        return Ok(());
    }
    if last {
        if let Some(mut ask) = state.asks.remove(&stream) {
            ask.frames.push(frame.bytes().to_vec());
            // An asker gone has no one to tell.
            drop(ask.reply.send(Ok(ask.frames)));
        }
        return Ok(());
    }
    if let Some(ask) = state.asks.get_mut(&stream) {
        ask.frames.push(frame.bytes().to_vec());
    }
    // Catalog and Defaults frames spend credit as Result frames do (`PROTOCOL.md` §13.7, §13.28): each frame read grants the
    // next.
    if view.command() == Command::Catalog || view.command() == Command::Defaults {
        let header = Header::new(state.cluster, Command::Credit).with_stream(stream);
        if let Ok(credit) = Credit::new(1).frame(header) {
            let sender = state.queue(credit);
            drop(state);
            if let Some(sender) = sender {
                sender.wake();
            }
        }
    }
    Ok(())
}

/// The refusal an Error frame holds.
///
/// # Errors
///
/// The rule its body breaks.
fn error_of(view: &FrameView<'_>) -> Result<ErrorReply, String> {
    ErrorReplyView::from_frame(view)
        .and_then(|error| error.to_body())
        .map_err(|rule| rule.to_string())
}

/// A frame of the subscription: a close, applied once whole, or the subscription's end, which opens it again.
///
/// # Errors
///
/// A command a Subscribe is not answered by, or a close or an Error whose body breaks its rules.
fn delivered_close(link: &SystemLink, view: &FrameView<'_>) -> Result<(), String> {
    if view
        .command()
        .answering(Command::Subscribe)
        .map_err(|rule| rule.to_string())?
        == Command::Error
    {
        let ended = error_of(view)?;
        tracing::debug!(
            reason = ended.message(),
            "the system group ended the subscription"
        );
        // Ended as no longer held: opened again with `latest`, skipping closes (`PROTOCOL.md` §13.3).
        let mut state = link.state.borrow_mut();
        let sender = subscribe(&mut state);
        state.view.reopened();
        drop(state);
        if let Some(sender) = sender {
            sender.wake();
        }
        return Ok(());
    }
    let close = EpochCloseView::from_frame(view).map_err(|rule| rule.to_string())?;
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
            return Ok(());
        }
        let parts = std::mem::take(&mut state.close_parts);
        let epoch = parts.first().map_or(close.epoch(), |part| part.0);
        let placement = parts.iter().map(|part| part.1).max().unwrap_or(0);
        let every_announced: Vec<(u64, u64)> = parts.into_iter().flat_map(|part| part.2).collect();
        if placement != 0 {
            state.placement_asked = placement;
        }
        // A close at or below the latest held applies nothing, and the view's waiters are told either way.
        let _applied = state.view.close(epoch, placement, &every_announced);
    }
    link.notify();
    link.refresh();
    Ok(())
}

/// A Check frame's reply: each check's outcome to its waiter, checked against what was sent, and its round trip counted; or
/// the group's refusal of the frame to each.
///
/// # Errors
///
/// A command a Check is not answered by, or a reply or an Error whose body breaks its rules, or a reply not of the checks
/// sent: the checks are left to the connection's close, which refuses them with code 17.
fn delivered_checks(
    link: &SystemLink,
    view: &FrameView<'_>,
    checks: &[(u64, SessionCheck)],
    sent: Instant,
) -> Result<(), String> {
    let answered = now();
    let outcomes = if view
        .command()
        .answering(Command::Check)
        .map_err(|rule| rule.to_string())?
        == Command::Check
    {
        let sent_checks: Vec<_> = checks.iter().map(|(_, check)| *check).collect();
        let reply = CheckView::from_frame(view)
            .and_then(|reply| reply.check_reply(&sent_checks).map(|()| reply))
            .map_err(|rule| rule.to_string())?;
        Ok((0..reply.len())
            .filter_map(|at| reply.get(at))
            .map(|check| check.outcome())
            .collect::<Vec<_>>())
    } else {
        Err(error_of(view)?)
    };
    let mut state = link.state.borrow_mut();
    state.round_trips.record(sent, answered);
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
        // A waiter gone has no one to tell.
        drop(waiting.send(outcome));
    }
    Ok(())
}

/// An answer read on connection `connection` refused by its asker for `rule`, which closes that connection as the side that
/// dialled refusing a frame does (`PROTOCOL.md` §2.6, §13), unless it has closed already: the refusal the asker answers, code
/// 17, as every request the connection carried is answered.
pub(super) fn refuse(link: &SystemLink, connection: u64, rule: &ProtocolError) -> ErrorReply {
    tracing::warn!(%rule, "the system group answered with a frame the link refuses; closing the connection");
    {
        let mut state = link.state.borrow_mut();
        if state.open && state.connection == connection {
            state.refused = true;
        }
    }
    link.notify();
    unavailable(
        "the connection to the system group closed under the request: its answer was refused",
    )
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
            // Queued or sent, the check is refused alike: one that left is answered by no reply on this connection.
            let _queued = state.turns.withdraw(ticket);
            if let Some(waiting) = state.waiting.remove(&ticket) {
                // A waiter gone has no one to tell, here and for each ask below.
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
