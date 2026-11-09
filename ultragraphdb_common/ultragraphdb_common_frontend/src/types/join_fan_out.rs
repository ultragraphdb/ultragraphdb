/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::mpsc::{UnboundedSender, unbounded};
use ultragraphdb_protocol_wire::Join;

use crate::types::core_joins::CoreJoins;

/// The way every endpoint core but the role listener's hears of a role that joined: core 0 registers each `Join` with its own
/// links and sends it here, to each other core, which applies it to its own (design D14). A clone sends to the same cores.
#[derive(Debug, Clone, Default)]
pub struct JoinFanOut {
    cores: Vec<UnboundedSender<Join>>,
}

impl JoinFanOut {
    /// A fan-out to no core yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One more core, which hears every join sent from here on through what this returns.
    pub fn add_core(&mut self) -> CoreJoins {
        let (sender, receiver) = unbounded();
        self.cores.push(sender);
        CoreJoins::new(receiver)
    }

    /// The cores it sends to.
    #[must_use]
    pub fn cores(&self) -> usize {
        self.cores.len()
    }

    /// Sends `join` to every core; a core that ended takes it no more, as the frontend is stopping.
    pub fn send(&self, join: &Join) {
        for core in &self.cores {
            let _sent = core.unbounded_send(join.clone());
        }
    }
}
