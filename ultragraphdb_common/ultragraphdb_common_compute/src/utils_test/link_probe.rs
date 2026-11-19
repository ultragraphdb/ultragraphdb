/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;
use std::future::poll_fn;
use std::task::Poll;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST,
};

use crate::types::counted_frame::CountedFrame;
use crate::types::link_event::LinkEvent;
use crate::types::role_link::RoleLink;
use crate::types::role_links::RoleLinks;
use crate::types::route_key::RouteKey;
use crate::types::route_sink::RouteSink;
use crate::types::session_event::SessionEvent;

/// What a stream on a probed link was told, in the order it was told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Told {
    /// A frame of this command answering a request of the stream.
    Frame(Command),
    /// The peer's Error on the stream, its code, which ends it.
    Refused(ErrorCode),
    /// The connection closed under the stream, or could not be opened: what the core answers with code 17.
    Closed,
}

/// A read's stream on the probed link: where its frames arrive, and whether its last has.
#[derive(Debug)]
struct ReadOpen {
    frames: UnboundedReceiver<ReceivedFrame>,
    ended: bool,
}

/// One core's role link to one partition, driven as the core's client sessions and reads drive it: a session's requests (a
/// Bind and what its stream carries, a one-frame write) opened and sent by their frames, a read's Scan or Closed opened by its
/// body and its frames taken whole, and what each stream was told. The link is the one `links` holds for the partition: once
/// it has closed, the next request takes the one `links` dials in its place, as a core's next request does.
#[derive(Debug)]
pub struct LinkProbe {
    links: RoleLinks,
    partition: u32,
    link: Option<RoleLink>,
    events: UnboundedSender<SessionEvent>,
    told: UnboundedReceiver<SessionEvent>,
    reads: HashMap<u64, ReadOpen>,
}

impl LinkProbe {
    /// The probe of `links`' link to `partition`.
    #[must_use]
    pub fn new(links: RoleLinks, partition: u32) -> Self {
        let (events, told) = unbounded();
        Self {
            links,
            partition,
            link: None,
            events,
            told,
            reads: HashMap::new(),
        }
    }

    /// The link the next request goes on: the one held while it is open, else the one `links` holds, dialled again once
    /// closed; `None` while nothing joined for the partition.
    fn link(&mut self) -> Option<RoleLink> {
        let held = self.link.as_ref().filter(|link| !link.is_closed()).cloned();
        if held.is_some() {
            return held;
        }
        self.link = self.links.partition(self.partition);
        self.link.clone()
    }

    /// The number of the stream the next request opens, on the link it goes on; `None` while nothing joined for the
    /// partition. A link that closed is replaced here: what its streams were told is the test's to take first.
    pub fn next_stream(&mut self) -> Option<u64> {
        self.link().map(|link| link.next_stream())
    }

    /// Opens a session's stream by its request `frame`, built on [`LinkProbe::next_stream`]: a Bind, a one-frame write. Its
    /// number; `None` while nothing joined for the partition.
    pub fn open(&mut self, frame: Vec<u8>) -> Option<u64> {
        let link = self.link()?;
        let stream = link.next_stream();
        let key = RouteKey::OneFrame { stream, attempt: 0 };
        let sink = RouteSink::new(self.events.clone(), key);
        Some(link.open(CountedFrame::uncounted(frame), sink))
    }

    /// Sends the request `frame` on the session's `stream`, already open: a Bind stream's Write or Commit.
    pub fn send(&self, stream: u64, frame: Vec<u8>) {
        if let Some(link) = &self.link {
            link.send(stream, CountedFrame::uncounted(frame));
        }
    }

    /// Sends on `stream` a frame shorter than a header, which no writer can number: its write fails, as one on a broken
    /// connection does.
    pub fn send_unwritable(&self, stream: u64) {
        self.send(stream, Vec::new());
    }

    /// Opens a read's stream by its request `body`, a Scan or a Closed: its number.
    ///
    /// # Errors
    ///
    /// What the link answers a request it could not send with: the rule its frame breaks, or code 17 for a closed link or a
    /// partition nothing joined for.
    pub fn open_read<B: BodyBuilder>(&mut self, body: &B) -> Result<u64, ErrorReply> {
        let Some(link) = self.link() else {
            return Err(ErrorReply::new(
                ErrorCode::Unavailable,
                format!("nothing joined for partition {}", self.partition),
            ));
        };
        let (stream, frames) = link.open_read(body)?;
        let _replaced = self.reads.insert(
            stream,
            ReadOpen {
                frames,
                ended: false,
            },
        );
        Ok(stream)
    }

    /// Opens a read's stream by `body` on the link it holds, open or closed, as a read that took the link before it closed
    /// does: its number.
    ///
    /// # Errors
    ///
    /// As [`LinkProbe::open_read`], code 17 for a probe holding no link yet.
    pub fn open_read_on_held<B: BodyBuilder>(&mut self, body: &B) -> Result<u64, ErrorReply> {
        let Some(link) = self.link.clone() else {
            return Err(ErrorReply::new(ErrorCode::Unavailable, "no link held yet"));
        };
        let (stream, frames) = link.open_read(body)?;
        let _replaced = self.reads.insert(
            stream,
            ReadOpen {
                frames,
                ended: false,
            },
        );
        Ok(stream)
    }

    /// Grants the read's `stream` one more frame.
    pub fn grant(&self, stream: u64) {
        if let Some(link) = &self.link {
            link.grant(stream);
        }
    }

    /// Cancels `stream`: nothing of it is told from here on.
    pub fn cancel(&mut self, stream: u64) {
        drop(self.reads.remove(&stream));
        if let Some(link) = &self.link {
            link.cancel(stream);
        }
    }

    /// How many streams the link it holds has open; 0 before its first request.
    #[must_use]
    pub fn streams_open(&self) -> usize {
        self.link.as_ref().map_or(0, RoleLink::streams_open)
    }

    /// Whether the link it holds has closed; `false` before its first request.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.link.as_ref().is_some_and(RoleLink::is_closed)
    }

    /// The next stream told something, and what: a read's frames in the order they arrived, a session's in the order its
    /// events came. It waits until one is; a test bounds the wait.
    /// A dropped future leaves what was not yet taken for the next call.
    pub async fn next_told(&mut self) -> (u64, Told) {
        poll_fn(|context| {
            if let Poll::Ready(Some(SessionEvent::Role { key, event })) =
                self.told.poll_next_unpin(context)
            {
                return Poll::Ready((stream_of(key), told_of(&event)));
            }
            let mut ended = None;
            for (stream, read) in &mut self.reads {
                match read.frames.poll_next_unpin(context) {
                    Poll::Ready(Some(frame)) => {
                        read.ended = frame.flags() & FLAG_LAST != 0;
                        return Poll::Ready((*stream, read_told(&frame)));
                    }
                    Poll::Ready(None) => {
                        ended = Some((*stream, read.ended));
                        break;
                    }
                    Poll::Pending => {}
                }
            }
            if let Some((stream, last_seen)) = ended {
                drop(self.reads.remove(&stream));
                if !last_seen {
                    return Poll::Ready((stream, Told::Closed));
                }
                // A stream ended by its last frame was told so with that frame: the next is looked for at once.
                context.waker().wake_by_ref();
            }
            Poll::Pending
        })
        .await
    }
}

/// The link's stream a session's key names.
fn stream_of(key: RouteKey) -> u64 {
    match key {
        RouteKey::OneFrame { stream, .. } | RouteKey::Drop { stream } => stream,
        RouteKey::Recall { stream, .. } | RouteKey::Claim { stream } => stream,
        RouteKey::Bind { transaction, .. } => transaction,
    }
}

/// What a session's stream was told by `event`.
fn told_of(event: &LinkEvent) -> Told {
    match event {
        LinkEvent::Frame(bytes) => {
            let command = bytes
                .get(54..56)
                .and_then(|number| number.try_into().ok())
                .map(u16::from_le_bytes)
                .and_then(|number| Command::from_number(number).ok())
                .unwrap_or(Command::Error);
            Told::Frame(command)
        }
        LinkEvent::Bound(_) => Told::Frame(Command::Bind),
        LinkEvent::Held => Told::Frame(Command::Held),
        LinkEvent::Registered(_) => Told::Frame(Command::Reference),
        LinkEvent::Refused(reply) => Told::Refused(reply.code()),
        LinkEvent::Closed => Told::Closed,
    }
}

/// What a read's stream was told by `frame`, by its header, an Error read for its code.
fn read_told(frame: &ReceivedFrame) -> Told {
    let command = Command::from_number(frame.command()).unwrap_or(Command::Error);
    if command != Command::Error {
        return Told::Frame(command);
    }
    frame
        .view()
        .ok()
        .and_then(|view| ErrorReplyView::from_frame(&view).ok())
        .map_or(Told::Frame(Command::Error), |error| {
            Told::Refused(error.code())
        })
}
