/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use ultragraphdb_common_role::{Answer, OpenRequests, Push, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{BodyView, ComputeReadView, ErrorReply};

use crate::types::compute_core::ComputeCore;
use crate::types::read_executor::ReadExecutor;
use crate::types::read_request::ReadRequest;

/// One frontend's connection at compute's role address: each ComputeRead runs on this core through the connection's
/// [`ReadExecutor`], its batches pushed on the read's stream, every Credit counted by the connection, a Cancel or the
/// connection's end dropping the read.
#[derive(Debug)]
pub struct ComputeSession {
    reads: ReadExecutor,
}

impl ComputeSession {
    /// The session of a connection of `core`.
    #[must_use]
    pub fn new(core: Rc<ComputeCore>) -> Self {
        Self {
            reads: ReadExecutor::new(core),
        }
    }

    /// The session counting each read until it ends among `open`, which a stop waits on.
    #[must_use]
    pub fn with_open(self, open: OpenRequests) -> Self {
        Self {
            reads: self.reads.with_open(open),
        }
    }
}

impl Session for ComputeSession {
    /// It holds no `.await`: the read's task is spawned before it returns, so a dropped future leaves nothing.
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let read = frame
            .view()
            .map_err(|rule| ErrorReply::protocol(&rule))
            .and_then(|view| {
                let read = ComputeReadView::from_frame(&view)
                    .map_err(|rule| ErrorReply::protocol(&rule))?;
                ReadRequest::of(&read, stream)
            });
        match read {
            Ok(read) => {
                self.reads.start(stream, read);
                Answer::Open
            }
            Err(refusal) => Answer::Refuse(refusal),
        }
    }

    /// A future dropped at its wait leaves the frames to come to the next call.
    async fn next_push(&mut self) -> (u64, Push) {
        let (stream, frame) = self.reads.next().await;
        (stream, Push::Frame(frame))
    }

    fn cancelled(&mut self, stream: u64) {
        self.reads.cancel(stream);
    }

    fn ready(&mut self, stream: u64) {
        self.reads.ready(stream);
    }
}
