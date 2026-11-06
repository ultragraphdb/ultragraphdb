/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::channel::oneshot;
use ultragraphdb_common_role::{Answer, ParkedBytes, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, FLAG_LAST, HeaderView, MAX_FRAME_FLOOR};

use crate::types::core_message::{CoreMessage, Pushes};

/// One connection at a partition's role address: every request goes to its core's task, which owns the partition.
#[derive(Debug)]
pub struct StorageSession {
    connection: u64,
    core: UnboundedSender<CoreMessage>,
    pushes: Pushes,
    pushed: UnboundedReceiver<(u64, Vec<u8>)>,
    /// Pushes taken off their channel by a look at what waits to leave, in order and ahead of the channel's.
    looked_at: VecDeque<(u64, Vec<u8>)>,
    parked: ParkedBytes,
    max_frame: u32,
}

impl StorageSession {
    /// The session of `connection`, handing its requests to `core`, which holds up to `held_bytes` of its frames parked before
    /// the connection stops reading.
    #[must_use]
    pub fn new(connection: u64, core: UnboundedSender<CoreMessage>, held_bytes: usize) -> Self {
        let (pushes, pushed) = unbounded();
        let parked = ParkedBytes::new(held_bytes);
        // A core gone holds nothing to park.
        drop(core.unbounded_send(CoreMessage::Opened {
            connection,
            parked: parked.clone(),
        }));
        Self {
            connection,
            core,
            pushes,
            pushed,
            looked_at: VecDeque::new(),
            parked,
            max_frame: MAX_FRAME_FLOOR as u32,
        }
    }

    /// Whether a frame ending `stream` waits to leave: a refusal of a request that crossed it on the wire says only that the
    /// transaction has gone, and the frame says why.
    fn ending(&mut self, stream: u64) -> bool {
        while let Ok(pushed) = self.pushed.try_recv() {
            self.looked_at.push_back(pushed);
        }
        self.looked_at.iter().any(|(on, frame)| {
            *on == stream
                && HeaderView::new(frame).is_ok_and(|header| header.flags() & FLAG_LAST != 0)
        })
    }
}

impl Session for StorageSession {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let (done, answer) = oneshot::channel();
        let handed = self.core.unbounded_send(CoreMessage::Request {
            connection: self.connection,
            stream,
            frame,
            done,
            pushes: self.pushes.clone(),
            max_frame: self.max_frame,
        });
        let stopped = || {
            Answer::Refuse(ErrorReply::new(
                ErrorCode::Unavailable,
                "the partition stopped",
            ))
        };
        if handed.is_err() {
            return stopped();
        }
        match answer.await.unwrap_or_else(|_| stopped()) {
            Answer::Refuse(_) if self.ending(stream) => Answer::Open,
            answered @ (Answer::Reply(_) | Answer::Open | Answer::Done | Answer::Refuse(_)) => {
                answered
            }
        }
    }

    fn parked(&self) -> Option<ParkedBytes> {
        Some(self.parked.clone())
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        if let Some(pushed) = self.looked_at.pop_front() {
            return pushed;
        }
        match self.pushed.next().await {
            Some(pushed) => pushed,
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
