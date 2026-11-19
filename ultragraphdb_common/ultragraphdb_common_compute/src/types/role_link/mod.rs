/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod role_link_tasks;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::task::Waker;
use std::time::Instant;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Cancel, Command, Credit, ErrorReply, Header, HeaderView, MAX_FRAME_SIZE_U32,
};

use crate::types::commit_hold::CommitHold;
use crate::types::counted_frame::CountedFrame;
use crate::types::link_event::LinkEvent;
use crate::types::route_sink::RouteSink;
use crate::utils::clock::now;
use crate::utils::refusals::unavailable;

/// A frame waiting to leave, by the stream it asks on.
#[derive(Debug)]
struct Outgoing {
    stream: u64,
    /// Whether it opens the stream, so nothing of the stream has left while it waits.
    opens: bool,
    frame: CountedFrame,
}

/// Who reads a stream: a client session, told what each frame says, or a read, handed each frame whole to check itself, its
/// frames that spend credit held to the partition's `initial_credit` and what it granted since (`PROTOCOL.md` §11.3).
#[derive(Debug)]
enum Reader {
    Session(RouteSink),
    Read {
        frames: UnboundedSender<ReceivedFrame>,
        spent: u32,
        granted: u32,
    },
}

/// A stream open on a link: who reads it, and the requests sent on it, which each frame on it answers (`PROTOCOL.md` §2.5,
/// step 3).
#[derive(Debug)]
struct Route {
    reader: Reader,
    /// The request that opened it.
    opened: Command,
    /// The other requests sent on it, each once: a Bind's stream carries its transaction's Writes, Operations and Commit, or
    /// its Drop.
    sent: Vec<Command>,
}

impl Route {
    /// Whether the stream is a read's whose reader went without letting it go: its frames are late.
    fn reader_gone(&self) -> bool {
        match &self.reader {
            Reader::Read { frames, .. } => frames.is_closed(),
            Reader::Session(_) => false,
        }
    }

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
    /// The waker of the task reading the connection, which a close ends.
    reader: Option<Waker>,
    /// The credit each read's stream starts with, the peer's Welcome's.
    initial_credit: u32,
    /// When the peer last sent a frame; `None` before its first.
    heard: Option<Instant>,
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
            reader: Reader::Session(sink),
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

    /// Opens the stream of a read's request `body`, a Scan or a Closed, built on the stream it opens: its number, and where its
    /// frames arrive whole and unread, for the read to check each once. The stream's frames that spend credit are held to the
    /// partition's Welcome's `initial_credit` and what [`RoleLink::grant`] adds, one past it closing the connection
    /// (`PROTOCOL.md` §11.3); a frame its request is not answered by closes it too (§2.5, step 3). The receiver ends when the
    /// stream does: by its last frame, its read forgetting it, or the connection's close, which a read takes as code 17.
    ///
    /// # Errors
    ///
    /// The rule the request's frame breaks, and code 17 on a closed connection: either way no stream is registered, and a
    /// frame arriving later on the number is a stray.
    pub(crate) fn open_read<B: BodyBuilder>(
        &self,
        body: &B,
    ) -> Result<(u64, UnboundedReceiver<ReceivedFrame>), ErrorReply> {
        let mut routes = self.routes.borrow_mut();
        if routes.closed {
            return Err(unavailable("the role connection closed under the read"));
        }
        let stream = routes.next;
        debug_assert!(stream != 0, "stream 0 is the connection's own");
        let frame = body
            .frame(self.header(B::COMMAND, stream))
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        routes.next = stream.saturating_add(1);
        let (frames, arriving) = unbounded();
        let route = Route {
            reader: Reader::Read {
                frames,
                spent: 0,
                granted: 0,
            },
            opened: B::COMMAND,
            sent: Vec::new(),
        };
        let _none = routes.open.insert(stream, route);
        let writer = routes.queue(stream, true, CountedFrame::uncounted(frame));
        drop(routes);
        if let Some(writer) = writer {
            writer.wake();
        }
        Ok((stream, arriving))
    }

    /// Grants the read's `stream` one more frame that spends credit: a Credit of one frame leaves on it. A stream ended, or
    /// one of a session, is granted nothing.
    pub(crate) fn grant(&self, stream: u64) {
        let granted = match self.routes.borrow_mut().open.get_mut(&stream) {
            Some(Route {
                reader: Reader::Read { granted, .. },
                ..
            }) => {
                *granted = granted.saturating_add(1);
                true
            }
            Some(Route {
                reader: Reader::Session(_),
                ..
            })
            | None => false,
        };
        // A Credit that cannot be built sends nothing, and the stream's frames stay within what left before.
        if granted && let Ok(frame) = Credit::new(1).frame(self.header(Command::Credit, stream)) {
            self.leave(stream, CountedFrame::uncounted(frame));
        }
    }

    /// How many streams are open on the link: each a request sent and not yet ended.
    pub(crate) fn streams_open(&self) -> usize {
        self.routes.borrow().open.len()
    }

    /// Closes the connection, as a frame the link refuses closes it: every stream on it is told, and its reader and writer end.
    pub(crate) fn close(&self) {
        role_link_tasks::closed(self);
    }

    /// When the peer last sent a frame on the connection; the dial's start before its first.
    pub(crate) fn heard(&self) -> Instant {
        self.routes.borrow().heard.unwrap_or_else(now)
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

    /// `frame` of `stream` waits to leave: a request, or a Cancel, which no frame answers.
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

    /// Keeps `hold` with `stream` until the stream ends, by its last frame or the connection's close; a stream already ended
    /// lets it go at once.
    pub(crate) fn hold(&self, stream: u64, hold: CommitHold) {
        if let Some(Route {
            reader: Reader::Session(sink),
            ..
        }) = self.routes.borrow_mut().open.get_mut(&stream)
        {
            sink.hold(hold);
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
