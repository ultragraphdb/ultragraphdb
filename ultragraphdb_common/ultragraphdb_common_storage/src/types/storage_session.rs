/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::{Answer, ParkedBytes, Push, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, MAX_FRAME_FLOOR};

use crate::types::core_message::{CoreMessage, Pushes};
use crate::types::parked_frame::ParkedFrame;
use crate::types::role_connection_phases::RoleConnectionPhases;

/// One connection at a partition's role address: every request goes to its core's task, which owns the partition. The session
/// copies each request out of the connection's receive buffer, hands the copy to the core and answers [`Answer::Open`] at once, so
/// the connection reads its next frame without waiting for the core to take the last (`PROTOCOL.md` §11.12, *Parked requests*):
/// what the connection carries while the core's turn waits on its log's sync reaches the core together. What it hands the core is
/// bounded by the connection's held bytes, its copies not yet taken together with those the core holds parked, at whose bound,
/// `storage.held_bytes`, the connection stops reading; and by one note of a stream's room, end, cancel or of its own close per
/// stream or connection. Every answer comes back as a push, in the order the core gives them.
#[derive(Debug)]
pub struct StorageSession {
    connection: u64,
    core: UnboundedSender<CoreMessage>,
    pushes: Pushes,
    pushed: UnboundedReceiver<(u64, Push)>,
    held: ParkedBytes,
    max_frame: u32,
    phases: Option<RoleConnectionPhases>,
    /// When each stream's requests still waiting for their first answer were read, in order: at most the requests the connection
    /// holds handed on, which `held_bytes` bounds.
    read_at: BTreeMap<u64, VecDeque<u64>>,
}

impl StorageSession {
    /// The session of `connection`, handing its requests to `core`, which holds up to `held_bytes` of its frames before the
    /// connection stops reading.
    #[must_use]
    pub fn new(connection: u64, core: UnboundedSender<CoreMessage>, held_bytes: usize) -> Self {
        // The core pushes, for each request the connection holds handed on, its answer, and for each open stream at most one frame
        // before that frame has left: a Scan's next waits for its Ready. Both are bounded by the connection, so the channel is.
        let (pushes, pushed) = unbounded();
        Self {
            connection,
            core,
            pushes,
            pushed,
            held: ParkedBytes::new(held_bytes),
            max_frame: MAX_FRAME_FLOOR as u32, // 2^19, lossless
            phases: None,
            read_at: BTreeMap::new(),
        }
    }

    /// The session timing each request from its read to its first answer taken, into `phases`.
    #[must_use]
    pub fn with_phases(mut self, phases: RoleConnectionPhases) -> Self {
        self.phases = Some(phases);
        self
    }

    /// Counts the first answer on `stream` taken now, if a request of it waits for one; a stream's end forgets the rest.
    fn answered(&mut self, stream: u64, push: &Push) {
        let Some(phases) = &self.phases else {
            return;
        };
        if let Some(read) = self.read_at.get_mut(&stream).and_then(VecDeque::pop_front) {
            phases.record(read, phases.now_us());
        }
        let ended = matches!(push, Push::End | Push::Refuse(_));
        if ended || self.read_at.get(&stream).is_some_and(VecDeque::is_empty) {
            let _forgotten = self.read_at.remove(&stream);
        }
    }
}

impl Session for StorageSession {
    /// Hands the request to the core and answers it as open: its answer is pushed. A frame out of order is refused here, as the
    /// connection's machine would refuse it on viewing it.
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        if let Err(rule) = frame.check_order() {
            return Answer::Refuse(ErrorReply::protocol(&rule));
        }
        let copy = ParkedFrame::copy(frame.bytes(), self.held.clone());
        // The connection's one buffer is free for its next frame.
        drop(frame);
        let handed = self.core.unbounded_send(CoreMessage::Request {
            connection: self.connection,
            stream,
            frame: copy,
            pushes: self.pushes.clone(),
            max_frame: self.max_frame,
        });
        if handed.is_err() {
            return Answer::Refuse(ErrorReply::new(
                ErrorCode::Unavailable,
                "the partition stopped",
            ));
        }
        if let Some(phases) = &self.phases {
            self.read_at
                .entry(stream)
                .or_default()
                .push_back(phases.now_us());
        }
        Answer::Open
    }

    fn parked(&self) -> Option<ParkedBytes> {
        Some(self.held.clone())
    }

    /// A future dropped at its wait leaves the channel as it was, and the next call takes what came meanwhile.
    async fn next_push(&mut self) -> (u64, Push) {
        match self.pushed.next().await {
            Some((stream, push)) => {
                self.answered(stream, &push);
                (stream, push)
            }
            // This session holds a sender, so its channel never ends while it waits here.
            None => std::future::pending().await,
        }
    }

    fn welcomed(&mut self, max_frame: u32) {
        self.max_frame = max_frame;
    }

    fn ready(&mut self, stream: u64) {
        // A core gone holds nothing to go on with.
        drop(self.core.unbounded_send(CoreMessage::Ready {
            connection: self.connection,
            stream,
        }));
    }

    fn left(&mut self, stream: u64) {
        // A core gone holds nothing to go on with.
        drop(self.core.unbounded_send(CoreMessage::Left {
            connection: self.connection,
            stream,
        }));
    }

    fn cancelled(&mut self, stream: u64) {
        let _forgotten = self.read_at.remove(&stream);
        // A core gone holds nothing to abort.
        drop(self.core.unbounded_send(CoreMessage::Cancelled {
            connection: self.connection,
            stream,
        }));
    }
}

impl Drop for StorageSession {
    fn drop(&mut self) {
        // A core gone holds nothing to abort.
        drop(self.core.unbounded_send(CoreMessage::Closed {
            connection: self.connection,
        }));
    }
}
