/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;
use std::rc::Rc;

use compio::runtime::{JoinHandle, spawn};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::{Answer, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{BodyView, ComputeReadView, ErrorReply};

use crate::types::compute_core::ComputeCore;
use crate::types::read_request::ReadRequest;
use crate::types::result_sink::{Pushed, ResultSink};
use crate::utils::run_read::run_read;

/// A read under way on a connection: its task, whose end cancels what it holds, and the signal of room for its next batch.
#[derive(Debug)]
struct OpenRead {
    _task: JoinHandle<()>,
    room: UnboundedSender<()>,
}

/// One frontend's connection at compute's role address: each ComputeRead runs as a task on this core, its batches pushed on the
/// read's stream, every Credit counted by the connection, a Cancel or the connection's end dropping the task.
#[derive(Debug)]
pub struct ComputeSession {
    core: Rc<ComputeCore>,
    reads: HashMap<u64, OpenRead>,
    pushes: UnboundedSender<Pushed>,
    pushed: UnboundedReceiver<Pushed>,
}

impl ComputeSession {
    /// The session of a connection of `core`.
    #[must_use]
    pub fn new(core: Rc<ComputeCore>) -> Self {
        let (pushes, pushed) = unbounded();
        Self {
            core,
            reads: HashMap::new(),
            pushes,
            pushed,
        }
    }
}

impl Session for ComputeSession {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let read = frame
            .view()
            .map_err(|rule| ErrorReply::protocol(&rule))
            .and_then(|view| {
                let read = ComputeReadView::from_frame(&view)
                    .map_err(|rule| ErrorReply::protocol(&rule))?;
                ReadRequest::of(&read, stream)
            });
        let read = match read {
            Ok(read) => read,
            Err(refusal) => return Answer::Refuse(refusal),
        };
        let (room, rooms) = unbounded();
        let sink = ResultSink::new(
            (self.core.cluster_id(), stream),
            read.max_frame,
            read.columns(),
            self.pushes.clone(),
            rooms,
        );
        let task = spawn(run_read(
            Rc::clone(&self.core),
            read,
            sink,
            self.pushes.clone(),
        ));
        let _new = self.reads.insert(stream, OpenRead { _task: task, room });
        Answer::Open
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        loop {
            match self.pushed.next().await {
                Some(Pushed::Frame(stream, frame)) => return (stream, frame),
                Some(Pushed::Ended(stream)) => drop(self.reads.remove(&stream)),
                // This session holds a sender, so its channel never ends while it waits here.
                None => std::future::pending::<()>().await,
            }
        }
    }

    fn cancelled(&mut self, stream: u64) {
        drop(self.reads.remove(&stream));
    }

    fn ready(&mut self, stream: u64) {
        if let Some(read) = self.reads.get(&stream) {
            // A read ended meanwhile wants no room.
            drop(read.room.unbounded_send(()));
        }
    }
}
