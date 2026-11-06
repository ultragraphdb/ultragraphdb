/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod role_link_tasks;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{BodyBuilder, Cancel, Command, Credit, Header, MAX_FRAME_SIZE};

use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;
use crate::types::session_event::SessionEvent;

/// Where a stream's frames go: its session's events, under the name the session knows it by.
#[derive(Debug, Clone)]
pub struct RouteSink {
    events: UnboundedSender<SessionEvent>,
    key: RouteKey,
}

impl RouteSink {
    /// The sink of `key` in the session reading `events`.
    pub fn new(events: UnboundedSender<SessionEvent>, key: RouteKey) -> Self {
        Self { events, key }
    }

    /// Tells the session `event`; a session gone has no one to tell.
    fn tell(&self, event: LinkEvent) {
        drop(self.events.unbounded_send(SessionEvent::Role {
            key: self.key,
            event,
        }));
    }
}

/// The streams of a link and who reads each.
#[derive(Debug, Default)]
struct Routes {
    next: u64,
    open: HashMap<u64, RouteSink>,
    closed: bool,
}

/// One role connection a core's client sessions share, dialled once: its writes go out by message and a task hands each frame
/// that arrives to the session whose stream it answers, so no session waits on another's frame. Shared by an `Rc`, on its one core.
/// When it closes, each stream on it is told, and a stream opened after that is told at once.
#[derive(Debug, Clone)]
pub struct RoleLink {
    cluster: u64,
    outgoing: UnboundedSender<Vec<u8>>,
    routes: Rc<RefCell<Routes>>,
    max_frame: Rc<Cell<u32>>,
}

impl RoleLink {
    /// The link to `address`, dialled as `name` holding `password` on a task of the core, what it is sent meanwhile kept in order.
    #[must_use]
    pub fn dial(address: Address, cluster: u64, name: String, password: String) -> Self {
        let (link, outgoing) = Self::parts(cluster);
        role_link_tasks::spawn_dialled(link.clone(), outgoing, address, name, password);
        link
    }

    /// The link over `client`, already dialled.
    #[must_use]
    pub fn over(client: RoleClient, cluster: u64) -> Self {
        let (link, outgoing) = Self::parts(cluster);
        role_link_tasks::spawn_over(link.clone(), outgoing, client);
        link
    }

    fn parts(cluster: u64) -> (Self, futures::channel::mpsc::UnboundedReceiver<Vec<u8>>) {
        let (sender, outgoing) = unbounded();
        let link = Self {
            cluster,
            outgoing: sender,
            routes: Rc::new(RefCell::new(Routes {
                next: 1,
                ..Routes::default()
            })),
            max_frame: Rc::new(Cell::new(MAX_FRAME_SIZE as u32)),
        };
        (link, outgoing)
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
    /// `sink` so at once. The stream's number.
    pub fn open(&self, frame: Vec<u8>, sink: RouteSink) -> u64 {
        let mut routes = self.routes.borrow_mut();
        let stream = routes.next;
        routes.next = stream.saturating_add(1);
        if routes.closed {
            drop(routes);
            sink.tell(LinkEvent::Closed);
            return stream;
        }
        let _none = routes.open.insert(stream, sink);
        drop(routes);
        self.send(frame);
        stream
    }

    /// Sends `frame` on a stream already open.
    pub fn send(&self, frame: Vec<u8>) {
        // A task gone leaves the routes told of the close.
        drop(self.outgoing.unbounded_send(frame));
    }

    /// Cancels `stream`, and no frame of it is told any more.
    pub fn cancel(&self, stream: u64) {
        self.forget(stream);
        if let Ok(frame) = Cancel.frame(self.header(Command::Cancel, stream)) {
            self.send(frame);
        }
    }

    /// Grants `stream` credit for `frames` more frames.
    pub fn credit(&self, stream: u64, frames: u32) {
        if let Ok(frame) = Credit::new(frames).frame(self.header(Command::Credit, stream)) {
            self.send(frame);
        }
    }

    /// Stops telling of `stream`: what arrives on it from here on is late.
    pub fn forget(&self, stream: u64) {
        let _gone = self.routes.borrow_mut().open.remove(&stream);
    }
}
