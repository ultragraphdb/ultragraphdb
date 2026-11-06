/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;
use std::rc::Rc;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::{Answer, OpenRequests, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{BodyView, ComputeReadView, ErrorReply};

use crate::types::compute_core::ComputeCore;
use crate::types::open_read::OpenRead;
use crate::types::pushed::Pushed;
use crate::types::read_request::ReadRequest;
use crate::types::result_sink::ResultSink;
use crate::utils::run_read::run_read;

/// One frontend's connection at compute's role address: each ComputeRead runs as a task on this core, its batches pushed on the
/// read's stream, every Credit counted by the connection, a Cancel or the connection's end dropping the task.
#[derive(Debug)]
pub struct ComputeSession {
    core: Rc<ComputeCore>,
    open: OpenRequests,
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
            open: OpenRequests::default(),
            reads: HashMap::new(),
            pushes,
            pushed,
        }
    }

    /// The session counting each read until it ends among `open`, which a stop waits on.
    #[must_use]
    pub fn with_open(mut self, open: OpenRequests) -> Self {
        self.open = open;
        self
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
        let open_read = OpenRead {
            _task: task,
            room,
            _open: self.open.enter(),
        };
        let _new = self.reads.insert(stream, open_read);
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
