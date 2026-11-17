/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::poll_fn;
use std::task::Poll;
use std::time::Duration;

use compio::runtime::spawn;
use compio::time::timeout;
use ultragraphdb_common_role::{RoleClient, RoleReader, RoleWriter};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BindView, BodyView, Command, CommitView, DropCommandView, ErrorReplyView, FLAG_LAST, FrameView,
    HeldView, ReferenceView,
};

use crate::types::link_event::LinkEvent;
use crate::types::role_link::RoleLink;

/// Dials within `dial_ms` milliseconds, then runs the link; a dial that fails or passes its bound closes it, telling every stream.
pub(super) fn spawn_dialled(
    link: RoleLink,
    address: Address,
    name: String,
    password: String,
    dial_ms: u32,
) {
    spawn(async move {
        let bound = Duration::from_millis(u64::from(dial_ms));
        match timeout(
            bound,
            RoleClient::dial(&address, link.cluster, &name, &password),
        )
        .await
        {
            Ok(Ok(client)) => run(link, client).await,
            Ok(Err(error)) => {
                tracing::debug!(address = %address, %error, "a role connection could not be opened");
                closed(&link);
            }
            Err(_) => {
                tracing::warn!("a role connection to {address} did not finish its connect, Hello and SCRAM exchange within {dial_ms} ms, transaction_ms, and counts as closed: check that the process at that address is the role it names and is not stuck");
                closed(&link);
            }
        }
    })
    .detach();
}

/// Runs the link over a connection already dialled.
pub(super) fn spawn_over(link: RoleLink, client: RoleClient) {
    spawn(run(link, client)).detach();
}

/// Writes on one task and reads on another, until either finds the connection closed.
/// A future dropped at the runtime's end drops the reader with a frame read in part.
async fn run(link: RoleLink, client: RoleClient) {
    let (reader, writer, welcome) = client.split();
    link.max_frame.set(welcome.max_frame());
    link.routes.borrow_mut().answered = true;
    spawn(write(link.clone(), writer)).detach();
    read(link, reader).await;
}

/// Writes each frame the link lets go until the link closes or a write fails, then shuts the sending side. A future
/// dropped at the runtime's end may cut the frame being written.
async fn write(link: RoleLink, mut writer: RoleWriter) {
    // An event loop: the link's close, which ends its outgoing frames, or a failed write ends it (Ultra rule 2).
    while let Some(frame) = next_outgoing(&link).await {
        if let Err(error) = writer.send(frame).await {
            tracing::debug!(%error, "a role connection's write failed");
            closed(&link);
            return;
        }
    }
    writer.shutdown().await;
}

/// The next frame to leave, which is no longer counted, once there is one; `None` once the connection closed.
/// A dropped future leaves the frame queued and its waker registered, which the next frame wakes for nothing.
async fn next_outgoing(link: &RoleLink) -> Option<Vec<u8>> {
    poll_fn(|cx| {
        let mut routes = link.routes.borrow_mut();
        if let Some(waiting) = routes.outgoing.pop_front() {
            return Poll::Ready(Some(waiting.frame.into_bytes()));
        }
        if routes.closed {
            return Poll::Ready(None);
        }
        routes.writer = Some(cx.waker().clone());
        Poll::Pending
    })
    .await
}

/// Hands every frame read to its stream's session until the connection ends or a frame is refused. A future dropped at
/// the runtime's end drops the reader with a frame read in part.
async fn read(link: RoleLink, mut reader: RoleReader) {
    // An event loop: the connection's end or a frame the link refuses ends it (Ultra rule 2).
    loop {
        let frame = match reader.next_frame().await {
            Ok(frame) => frame,
            Err(error) => {
                tracing::debug!(%error, "a role connection's read ended");
                closed(&link);
                return;
            }
        };
        if let Err(refused) = deliver(&link, &frame) {
            tracing::warn!(%refused, "a role connection sent what it may not, and closes");
            closed(&link);
            return;
        }
    }
}

/// Hands `frame` to the session whose stream it answers, read: a frame on a stream that has ended is late, and dropped. A
/// frame the link refuses ends the connection, as the side that dialled refusing a frame does (`PROTOCOL.md` §2.6, §13): one on
/// a stream not yet opened, which answers nothing, or by a command no request of its stream is answered by (§2.5, step 3); a
/// Result or Entries frame past its stream's credit, which breaks the rule that meters them (§11.3); a control reply whose body
/// breaks its command's rules (§2.5, step 16).
fn deliver(link: &RoleLink, frame: &ReceivedFrame) -> Result<(), String> {
    let view = frame.view().map_err(|rule| rule.to_string())?;
    let stream = view.header().stream();
    let command = view.command();
    let mut routes = link.routes.borrow_mut();
    if stream == 0 || stream >= routes.next {
        return Err(format!(
            "a frame on stream {stream}, which it holds no request on"
        ));
    }
    let Some(route) = routes.open.get_mut(&stream) else {
        return Ok(());
    };
    if !route.answers(command) {
        return Err(ProtocolError::command_not_served(command.number()).to_string());
    }
    let metered = matches!(command, Command::Result | Command::Entries);
    if metered && !route.sink.spend_credit() {
        return Err(ProtocolError::past_credit(stream).to_string());
    }
    let event = event_of(&view, frame).map_err(|rule| rule.to_string())?;
    let last = view.header().flags() & FLAG_LAST != 0;
    let sink = if last {
        routes.open.remove(&stream).map(|ended| ended.sink)
    } else {
        routes.open.get(&stream).map(|open| open.sink.clone())
    };
    drop(routes);
    if let Some(sink) = sink {
        sink.tell(event);
    }
    Ok(())
}

/// What `frame`, validated as `view` and answering a request of its stream, tells the stream's session: a control reply read, its
/// body checked by its command's rules; a Commit's or a Drop's reply checked so and relayed as it came; a Result relayed as it
/// came, whose chunks and body the client checks.
///
/// # Errors
///
/// The rule its body breaks; a Reference that is no reply; a command the link reads no frame of, which answers no request it
/// sends.
fn event_of(view: &FrameView<'_>, frame: &ReceivedFrame) -> Result<LinkEvent, ProtocolError> {
    let command = view.command();
    if command == Command::Result {
        return Ok(LinkEvent::Frame(frame.bytes().to_vec()));
    }
    if command == Command::Error {
        return Ok(LinkEvent::Refused(
            ErrorReplyView::from_frame(view)?.to_body()?,
        ));
    }
    if command == Command::Bind {
        return Ok(LinkEvent::Bound(BindView::from_frame(view)?.read_version()));
    }
    if command == Command::Held {
        let _held = HeldView::from_frame(view)?;
        return Ok(LinkEvent::Held);
    }
    if command == Command::Commit {
        let _reply = CommitView::from_frame(view)?;
        return Ok(LinkEvent::Frame(frame.bytes().to_vec()));
    }
    if command == Command::Drop {
        let _reply = DropCommandView::from_frame(view)?;
        return Ok(LinkEvent::Frame(frame.bytes().to_vec()));
    }
    if command == Command::Reference {
        let reply = ReferenceView::from_frame(view)?;
        // A Reference holds at least one registration by its rules (`PROTOCOL.md` §13.15).
        return match reply.registration(0).filter(|_| reply.is_reply()) {
            Some(registration) => Ok(LinkEvent::Registered(registration)),
            None => Err(ProtocolError::field_value(command.number(), "reply", 0)),
        };
    }
    Err(ProtocolError::command_not_served(command.number()))
}

/// Marks the connection closed, lets go of what waits to leave and tells each stream on it; the write task, finding it closed, shuts
/// the connection down.
fn closed(link: &RoleLink) {
    let mut routes = link.routes.borrow_mut();
    routes.closed = true;
    routes.outgoing.clear();
    let writer = routes.writer.take();
    let sinks: Vec<_> = routes.open.drain().map(|(_, route)| route.sink).collect();
    drop(routes);
    if let Some(writer) = writer {
        writer.wake();
    }
    for sink in sinks {
        sink.tell(LinkEvent::Closed);
    }
}
