/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::poll_fn;
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

use compio::runtime::spawn;
use compio::time::timeout;
use futures::future::{Either, select};
use ultragraphdb_common_role::{RoleClient, RoleReader, RoleWriter};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Address, BufferPool, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BindView, BodyView, Command, CommitView, DropCommandView, ErrorReplyView, FLAG_LAST, FrameView,
    HeldView, MAX_FRAME_SIZE, ReferenceView,
};

use crate::types::link_event::LinkEvent;
use crate::types::role_link::{Reader, RoleLink, Route};
use crate::utils::clock::now;
use crate::utils::yield_once::yield_once;

/// A link's frame buffers at a time: the reader goes on past this one from a new pool while a read holds a frame, so a read
/// that does not consume holds up no other and an idle link keeps one buffer resident.
const LINK_BUFFERS: usize = 1;

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
    {
        let mut routes = link.routes.borrow_mut();
        routes.answered = true;
        routes.initial_credit = welcome.initial_credit();
        routes.heard = Some(now());
    }
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
    poll_fn(|context| {
        let mut routes = link.routes.borrow_mut();
        if let Some(waiting) = routes.outgoing.pop_front() {
            return Poll::Ready(Some(waiting.frame.into_bytes()));
        }
        if routes.closed {
            return Poll::Ready(None);
        }
        routes.writer = Some(context.waker().clone());
        Poll::Pending
    })
    .await
}

/// Hands every frame read to its stream's reader until the connection ends, a frame is refused or the link closes. A read's
/// frame holds its buffer until the read takes it, so a new pool is taken once the core has run and the buffer is still held.
/// A future dropped at the runtime's end drops the reader with a frame read in part.
async fn read(link: RoleLink, mut reader: RoleReader) {
    let mut pool = BufferPool::new(LINK_BUFFERS, MAX_FRAME_SIZE);
    // An event loop: the connection's end, a frame the link refuses or the link's close ends it (Ultra rule 2).
    loop {
        if pool.available() == 0 {
            yield_once().await;
        }
        if pool.available() == 0 {
            pool = BufferPool::new(LINK_BUFFERS, MAX_FRAME_SIZE);
        }
        let next = pin!(reader.next_frame_in(&pool));
        let frame = match select(next, pin!(closing(&link))).await {
            Either::Left((Ok(frame), _)) => frame,
            Either::Left((Err(error), _)) => {
                tracing::debug!(%error, "a role connection's read ended");
                closed(&link);
                return;
            }
            // The link closed: its reader ends, and the connection with it.
            Either::Right(_) => return,
        };
        link.routes.borrow_mut().heard = Some(now());
        if let Err(refused) = deliver(&link, frame) {
            tracing::warn!(%refused, "a role connection sent what it may not, and closes");
            closed(&link);
            return;
        }
    }
}

/// Waits until the link is closed. A dropped future leaves its waker, which the close wakes for nothing.
async fn closing(link: &RoleLink) {
    poll_fn(|context| {
        let mut routes = link.routes.borrow_mut();
        if routes.closed {
            return Poll::Ready(());
        }
        routes.reader = Some(context.waker().clone());
        Poll::Pending
    })
    .await;
}

/// Hands `frame` to the reader of the stream it answers. A frame the link refuses ends the connection, as the side that dialled
/// refusing a frame does (`PROTOCOL.md` §2.6, §13): one out of its place on the connection; one on stream 0 or on a stream not
/// yet opened, which answers nothing, or by a command no request of its stream is answered by (§2.5, step 3); a frame past its
/// stream's credit, which breaks the rule that meters them (§11.3); a control reply whose body breaks its command's rules
/// (§2.5, step 16). A frame on a stream that has ended is late: checked whole and dropped, as no reader will check it. A read's
/// frame goes on whole, its body the read's to check, once.
fn deliver(link: &RoleLink, frame: ReceivedFrame) -> Result<(), String> {
    frame.check_order().map_err(|rule| rule.to_string())?;
    let stream = frame.stream();
    let mut routes = link.routes.borrow_mut();
    if stream == 0 || stream >= routes.next {
        return Err(format!(
            "a frame on stream {stream}, which it holds no request on"
        ));
    }
    let command = Command::from_number(frame.command()).map_err(|rule| rule.to_string())?;
    let initial = routes.initial_credit;
    let held = routes
        .open
        .get_mut(&stream)
        .filter(|route| !route.reader_gone());
    let Some(route) = held else {
        drop(routes.open.remove(&stream));
        drop(routes);
        return late(link, &frame);
    };
    if !route.answers(command) {
        return Err(ProtocolError::command_not_served(command.number()).to_string());
    }
    let last = frame.flags() & FLAG_LAST != 0;
    if let Reader::Read {
        frames,
        spent,
        granted,
    } = &mut route.reader
    {
        if command.spends_credit() {
            if *spent >= initial.saturating_add(*granted) {
                return Err(ProtocolError::past_credit(stream).to_string());
            }
            *spent = spent.saturating_add(1);
        }
        // The reader is there, as looked above, and nothing on this core runs between.
        drop(frames.unbounded_send(frame));
        if last {
            drop(routes.open.remove(&stream));
        }
        return Ok(());
    }
    drop(routes);
    deliver_to_session(link, stream, &frame)
}

/// A frame on a stream no one reads, checked whole and dropped.
fn late(link: &RoleLink, frame: &ReceivedFrame) -> Result<(), String> {
    FrameView::validate(frame.bytes(), link.cluster, MAX_FRAME_SIZE)
        .map(drop)
        .map_err(|rule| rule.to_string())
}

/// Hands `frame`, on the session's `stream`, to the session, read: its body checked by its command's rules, and a Result
/// within its stream's credit.
fn deliver_to_session(link: &RoleLink, stream: u64, frame: &ReceivedFrame) -> Result<(), String> {
    let view = frame.view().map_err(|rule| rule.to_string())?;
    let command = view.command();
    let mut routes = link.routes.borrow_mut();
    let Some(Route {
        reader: Reader::Session(sink),
        ..
    }) = routes.open.get_mut(&stream)
    else {
        return Ok(());
    };
    let metered = matches!(command, Command::Result | Command::Entries);
    if metered && !sink.spend_credit() {
        return Err(ProtocolError::past_credit(stream).to_string());
    }
    let event = event_of(&view, frame).map_err(|rule| rule.to_string())?;
    let last = view.header().flags() & FLAG_LAST != 0;
    let told = if last {
        routes
            .open
            .remove(&stream)
            .and_then(|ended| match ended.reader {
                Reader::Session(ended_sink) => Some(ended_sink),
                Reader::Read { .. } => None,
            })
    } else {
        Some(sink.clone())
    };
    drop(routes);
    if let Some(told) = told {
        told.tell(event);
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

/// Marks the connection closed, lets go of what waits to leave and tells each stream on it: a session's is told, a read's
/// frames end; the write task, finding it closed, shuts the connection down, and the read task ends.
pub(super) fn closed(link: &RoleLink) {
    let mut routes = link.routes.borrow_mut();
    routes.closed = true;
    routes.outgoing.clear();
    let writer = routes.writer.take();
    let reader = routes.reader.take();
    let sinks: Vec<_> = routes
        .open
        .drain()
        .filter_map(|(_, route)| match route.reader {
            Reader::Session(sink) => Some(sink),
            // A read's channel ends as its sender drops here: the read takes it as the connection's close.
            Reader::Read { .. } => None,
        })
        .collect();
    drop(routes);
    if let Some(writer) = writer {
        writer.wake();
    }
    if let Some(reader) = reader {
        reader.wake();
    }
    for sink in sinks {
        sink.tell(LinkEvent::Closed);
    }
}
