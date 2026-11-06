/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::mpsc::UnboundedSender;
use ultragraphdb_protocol_wire::Command;

/// A frame a double took, or what it was told: a Cancel or a Credit stands for what the connection told its session.
#[derive(Debug, Clone)]
pub struct DoubleFrame {
    stream: u64,
    command: Command,
    bytes: Vec<u8>,
    pushes: UnboundedSender<(u64, Vec<u8>)>,
}

impl DoubleFrame {
    /// The frame `bytes` of `command` taken on `stream`, answered through `pushes`.
    pub(crate) fn new(
        stream: u64,
        command: Command,
        bytes: Vec<u8>,
        pushes: UnboundedSender<(u64, Vec<u8>)>,
    ) -> Self {
        Self {
            stream,
            command,
            bytes,
            pushes,
        }
    }

    /// The stream it arrived on.
    #[must_use]
    pub fn stream(&self) -> u64 {
        self.stream
    }

    /// Its command.
    #[must_use]
    pub fn command(&self) -> Command {
        self.command
    }

    /// Its bytes; empty for what the connection told.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Sends `frame` back on this frame's stream.
    pub fn answer_with(&self, frame: Vec<u8>) {
        // A connection gone has no one to tell.
        drop(self.pushes.unbounded_send((self.stream, frame)));
    }
}
