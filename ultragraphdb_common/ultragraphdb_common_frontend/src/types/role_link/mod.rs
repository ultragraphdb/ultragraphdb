/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod role_link_tasks;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::task::Waker;

use ultragraphdb_common_role::{ParkedBytes, RoleClient};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Cancel, Command, Credit, Header, HeaderView, MAX_FRAME_SIZE_U32,
};

use crate::types::commit_hold::CommitHold;
use crate::types::counted_frame::CountedFrame;
use crate::types::link_event::LinkEvent;
use crate::types::route_sink::RouteSink;

/// A frame waiting to leave, by the stream it asks on.
#[derive(Debug)]
struct Outgoing {
    stream: u64,
    /// Whether it opens the stream, so nothing of the stream has left while it waits.
    opens: bool,
    frame: CountedFrame,
}

/// A stream open on a link: who reads it, and the requests sent on it, which each frame on it answers (`PROTOCOL.md` §2.5,
/// step 3).
#[derive(Debug)]
struct Route {
    sink: RouteSink,
    /// The request that opened it.
    opened: Command,
    /// The other requests sent on it, each once: a Bind's stream carries its transaction's Writes, Operations and Commit, or
    /// its Drop.
    sent: Vec<Command>,
}

impl Route {
    /// Whether a frame of `command` answers a request sent on the stream: an Error, or a command one of them is answered by.
    fn answers(&self, command: Command) -> bool {
        std::iter::once(&self.opened)
            .chain(&self.sent)
            .any(|request| command.answering(*request).is_ok())
    }
}

/// The command `frame` holds, by its header.
fn command_of(frame: &CountedFrame) -> Option<Command> {
    let header = HeaderView::new(frame.bytes()).ok()?;
    Command::from_number(header.command()).ok()
}

/// The streams of a link and who reads each, and what waits to leave, first in, first out.
#[derive(Debug, Default)]
struct Routes {
    next: u64,
    /// One for each stream a session opened that has not ended, so the sessions' own streams bound it (Ultra rule 2).
    open: HashMap<u64, Route>,
    closed: bool,
    /// Whether the peer answered the dial: its Welcome and its proof of the secret came.
    answered: bool,
    /// Each frame is counted in its client connection's bytes on their way, whose bound stops that connection's reads, so
    /// the connections' bounds limit its length.
    outgoing: VecDeque<Outgoing>,
    writer: Option<Waker>,
}

impl Routes {
    /// `frame` waits to leave, unless the connection closed. The waker to wake, for after the borrow.
    fn queue(&mut self, stream: u64, opens: bool, frame: CountedFrame) -> Option<Waker> {
        if self.closed {
            return None;
        }
        self.outgoing.push_back(Outgoing {
            stream,
            opens,
            frame,
        });
        self.writer.take()
    }
}

/// One role connection a core's client sessions share, dialled once: its writes wait in a queue one task sends from, and a task hands
/// each frame that arrives to the session whose stream it answers, so no session waits on another's frame. Shared by an `Rc`, on its one
/// core. What waits in the queue is counted against its client connection's bound, and a stream that ends takes its frames back
/// (`PROTOCOL.md` §11.12, *Frames on their way*). When it closes, each stream on it is told, and a stream opened after that is told at once.
///
/// Data plane (Ultra rule 3): a frame queued is the `Vec` its builder or the client's connection gave, written as it
/// is.
#[derive(Debug, Clone)]
pub struct RoleLink {
    cluster: u64,
    routes: Rc<RefCell<Routes>>,
    max_frame: Rc<Cell<u32>>,
}

impl RoleLink {
    /// The link to `address`, dialled as `name` holding `password` on a task of the core, what it is sent meanwhile kept in order.
    /// A dial past `dial_ms` milliseconds closes it.
    #[must_use]
    pub fn dial(
        address: Address,
        cluster: u64,
        name: String,
        password: String,
        dial_ms: u32,
    ) -> Self {
        let link = Self::parts(cluster);
        role_link_tasks::spawn_dialled(link.clone(), address, name, password, dial_ms);
        link
    }

    /// The link over `client`, already dialled.
    #[must_use]
    pub fn over(client: RoleClient, cluster: u64) -> Self {
        let link = Self::parts(cluster);
        role_link_tasks::spawn_over(link.clone(), client);
        link
    }

    /// A link of `cluster` holding no stream, its first numbered 1: stream 0 is the connection's own.
    fn parts(cluster: u64) -> Self {
        Self {
            cluster,
            routes: Rc::new(RefCell::new(Routes {
                next: 1,
                ..Routes::default()
            })),
            max_frame: Rc::new(Cell::new(MAX_FRAME_SIZE_U32)),
        }
    }

    /// The largest frame the peer takes, by its Welcome once it is known.
    #[must_use]
    pub fn max_frame(&self) -> u32 {
        self.max_frame.get()
    }

    /// Whether the connection has closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.routes.borrow().closed
    }

    /// Whether the peer answers: it answered the dial, and the connection has not closed since.
    #[must_use]
    pub fn answers(&self) -> bool {
        let routes = self.routes.borrow();
        routes.answered && !routes.closed
    }

    /// A header of `command` on `stream` of this connection.
    #[must_use]
    pub fn header(&self, command: Command, stream: u64) -> Header {
        Header::new(self.cluster, command).with_stream(stream)
    }

    /// The number of the stream the next request opens.
    #[must_use]
    pub fn next_stream(&self) -> u64 {
        self.routes.borrow().next
    }

    /// Opens the stream `frame`, built on [`RoleLink::next_stream`], asks on, whose frames go to `sink`; a closed connection tells
    /// `sink` so at once, as does a frame naming no command, which nothing could answer. The stream's number.
    pub(crate) fn open(&self, frame: CountedFrame, sink: RouteSink) -> u64 {
        let mut routes = self.routes.borrow_mut();
        let stream = routes.next;
        debug_assert!(stream != 0, "stream 0 is the connection's own");
        routes.next = stream.saturating_add(1);
        let opened = command_of(&frame);
        let Some(opened) = opened.filter(|_| !routes.closed) else {
            drop(routes);
            sink.tell(LinkEvent::Closed);
            return stream;
        };
        let route = Route {
            sink,
            opened,
            sent: Vec::new(),
        };
        let _none = routes.open.insert(stream, route);
        let writer = routes.queue(stream, true, frame);
        drop(routes);
        if let Some(writer) = writer {
            writer.wake();
        }
        stream
    }

    /// Sends the request `frame` on `stream`, already open, which the stream's frames may answer from here on.
    pub(crate) fn send(&self, stream: u64, frame: CountedFrame) {
        if let (Some(route), Some(command)) = (
            self.routes.borrow_mut().open.get_mut(&stream),
            command_of(&frame),
        ) && route.opened != command
            && !route.sent.contains(&command)
        {
            route.sent.push(command);
        }
        self.leave(stream, frame);
    }

    /// `frame` of `stream` waits to leave: a request, or a Credit or a Cancel, which no frame answers.
    fn leave(&self, stream: u64, frame: CountedFrame) {
        let writer = self.routes.borrow_mut().queue(stream, false, frame);
        if let Some(writer) = writer {
            writer.wake();
        }
    }

    /// Cancels `stream`, and no frame of it is told any more: what it has waiting never leaves, and a stream none of which left has
    /// nothing at its peer to cancel.
    pub fn cancel(&self, stream: u64) {
        if self.forget(stream) {
            return;
        }
        // A Cancel that cannot be built sends nothing: the stream is forgotten here either way.
        if let Ok(frame) = Cancel.frame(self.header(Command::Cancel, stream)) {
            self.leave(stream, CountedFrame::uncounted(frame));
        }
    }

    /// Grants `stream` credit for `frames` more frames, counted here before they can be spent, its frame counted in `count`.
    pub fn credit(&self, stream: u64, frames: u32, count: &ParkedBytes) {
        if let Some(route) = self.routes.borrow_mut().open.get_mut(&stream) {
            route.sink.grant_credit(frames);
        }
        if let Ok(frame) = Credit::new(frames).frame(self.header(Command::Credit, stream)) {
            self.leave(stream, CountedFrame::new(frame, count));
        }
    }

    /// Keeps `hold` with `stream` until the stream ends, by its last frame or the connection's close; a stream already ended
    /// lets it go at once.
    pub(crate) fn hold(&self, stream: u64, hold: CommitHold) {
        if let Some(route) = self.routes.borrow_mut().open.get_mut(&stream) {
            route.sink.hold(hold);
        }
    }

    /// Stops telling of `stream`, what arrives on it from here on being late, and takes back what it has waiting: whether none of
    /// the stream has left.
    pub fn forget(&self, stream: u64) -> bool {
        let mut routes = self.routes.borrow_mut();
        let _gone = routes.open.remove(&stream);
        let mut unopened = false;
        routes.outgoing.retain(|waiting| {
            if waiting.stream != stream {
                return true;
            }
            unopened |= waiting.opens;
            false
        });
        unopened
    }

    /// Takes `stream` back when none of it has left: whether it did. A stream part of which has left stays as it is.
    pub fn take_back(&self, stream: u64) -> bool {
        let unopened = self
            .routes
            .borrow()
            .outgoing
            .iter()
            .any(|waiting| waiting.stream == stream && waiting.opens);
        unopened && self.forget(stream)
    }
}
