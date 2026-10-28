/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;
use std::task::{Context, Poll};

use crate::types::sim_network::NetworkState;

/// One end of a [`crate::SimNetwork`] connection, the network's model of a byte stream, sans-I/O: a runtime's shell
/// adapts it to its reader and writer traits (`ultragraphdb_protocol_transport_compio` for compio). A send is one
/// message and wakes the peer's waiting read. A read takes from what has arrived, at most one arrived chunk; with
/// nothing arrived it advances the network's clock while anything is in flight, reads the end once the peer has shut
/// down its side and everything it sent has been read, and otherwise waits until the peer sends. A message the network
/// dropped never arrives, so a read waiting for it waits forever, as a client waits for a reply lost with its
/// connection.
#[derive(Debug, Clone)]
pub struct SimStream {
    state: Rc<RefCell<NetworkState>>,
    incoming: usize,
    outgoing: usize,
}

impl SimStream {
    pub(crate) fn new(state: Rc<RefCell<NetworkState>>, incoming: usize, outgoing: usize) -> Self {
        Self {
            state,
            incoming,
            outgoing,
        }
    }

    /// The bytes arrived and not yet read.
    pub fn pending(&self) -> usize {
        self.state
            .borrow()
            .links
            .get(self.incoming)
            .map_or(0, |link| link.arrived.iter().map(Vec::len).sum())
    }

    /// Sends `message` to the peer as one message, which the network's faults may drop, duplicate, delay or split.
    pub fn send(&self, message: Vec<u8>) {
        self.state.borrow_mut().send(self.outgoing, message);
    }

    /// Shuts down this end's side: the peer reads the end once it has read what was sent before.
    pub fn shutdown(&self) {
        if let Some(link) = self.state.borrow_mut().links.get_mut(self.outgoing) {
            link.closed = true;
            if let Some(reader) = link.reader.take() {
                reader.wake();
            }
        }
    }

    /// The next bytes arrived, at most `room`: `Some` of at most one arrived chunk, the rest kept for the next read;
    /// `None` for the end of the stream, or for a read with no room. With nothing arrived it steps the network's clock
    /// while a message is in flight on its link, and otherwise registers `cx`'s waker and is pending.
    pub fn poll_chunk(&self, cx: &mut Context<'_>, room: usize) -> Poll<Option<Vec<u8>>> {
        let mut state = self.state.borrow_mut();
        loop {
            let arrived = state
                .links
                .get(self.incoming)
                .is_some_and(|link| !link.arrived.is_empty());
            if room == 0 {
                return Poll::Ready(None);
            }
            if arrived {
                break;
            }
            if state
                .links
                .get(self.incoming)
                .is_some_and(|link| !link.in_flight.is_empty())
            {
                state.step();
                continue;
            }
            let Some(link) = state.links.get_mut(self.incoming) else {
                return Poll::Ready(None);
            };
            if link.closed {
                return Poll::Ready(None);
            }
            link.reader = Some(cx.waker().clone());
            return Poll::Pending;
        }
        let Some(link) = state.links.get_mut(self.incoming) else {
            return Poll::Ready(None);
        };
        let Some(mut chunk) = link.arrived.pop_front() else {
            return Poll::Ready(None);
        };
        if chunk.len() > room {
            let rest = chunk.split_off(room);
            link.arrived.push_front(rest);
        }
        Poll::Ready(Some(chunk))
    }
}
