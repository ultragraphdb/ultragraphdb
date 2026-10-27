/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sim_stream_async_read;
mod sim_stream_async_write;

use std::cell::RefCell;
use std::rc::Rc;

use crate::types::sim_network::NetworkState;

/// One end of a [`crate::SimNetwork`] connection, a compio stream. A write sends one message and wakes the peer's
/// waiting read. A read takes from what has arrived, at most one arrived chunk; with nothing arrived it advances the
/// network's clock while anything is in flight, returns zero bytes once the peer has shut down its side and
/// everything it sent has been read, and otherwise waits until the peer writes. A message the network dropped never
/// arrives, so a read waiting for it waits forever, as a client waits for a reply lost with its connection.
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
}
