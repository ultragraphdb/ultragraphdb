/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;
use std::rc::Rc;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::OpenRequests;
use ultragraphdb_protocol_wire::Header;

use crate::types::compute_core::ComputeCore;
use crate::types::open_read::OpenRead;
use crate::types::pushed::Pushed;
use crate::types::read_request::ReadRequest;
use crate::types::result_sink::ResultSink;
use crate::utils::run_read::run_read;

/// The reads of one connection, each a task of the core it runs on (design D3): a session hands it a read as a
/// [`ReadRequest`] and takes the read's frames back from [`ReadExecutor::next`], its Result batches and, flagged `LAST`, its
/// last batch or its refusal. Room for a read's next batch and a read's end go forward by call; dropped, it cancels every read
/// it holds. A read's task gives its core back after each Entries frame it consumes and each Result frame it builds.
///
/// It numbers each read it starts, as a role link numbers its streams, so what a read it no longer holds sends late is told
/// from what the read started after it on the same stream sends.
///
/// Data plane (Ultra rule 3): its frames are the `Vec`s the wire's builders return, handed on whole.
#[derive(Debug)]
pub struct ReadExecutor {
    /// The core's, shared by an `Rc` with every session and read on this core, so their reads share its links (Ultra rule 9).
    core: Rc<ComputeCore>,
    /// The count of open requests a stop waits on, each read counted until it ends; `None` for a session counting its reads
    /// itself.
    open: Option<OpenRequests>,
    /// Each read under way by its number: the session's stream and the read's task. The session's streams, at most its
    /// connection's `max_streams`, bound it.
    reads: HashMap<u64, (u64, OpenRead)>,
    /// The number of the read under way on each of the session's streams.
    streams: HashMap<u64, u64>,
    /// The number the next read takes.
    next: u64,
    /// Each read pushes one batch at a time, the next only once the one before has left, and then its end: the reads held, at
    /// most the connection's `max_streams`, bound the channel's length.
    pushes: UnboundedSender<Pushed>,
    pushed: UnboundedReceiver<Pushed>,
}

impl ReadExecutor {
    /// The reads of a connection on `core`.
    #[must_use]
    pub fn new(core: Rc<ComputeCore>) -> Self {
        let (pushes, pushed) = unbounded();
        Self {
            core,
            open: None,
            reads: HashMap::new(),
            streams: HashMap::new(),
            next: 1,
            pushes,
            pushed,
        }
    }

    /// The executor counting each read until it ends among `open`, which a stop waits on.
    #[must_use]
    pub fn with_open(mut self, open: OpenRequests) -> Self {
        self.open = Some(open);
        self
    }

    /// Starts `read` as the read of the session's `stream`, its first Result frame sent without waiting; a read still under way
    /// on `stream` is cancelled first.
    pub fn start(&mut self, stream: u64, mut read: ReadRequest) {
        self.cancel(stream);
        let number = self.next;
        self.next = self.next.saturating_add(1);
        read.stream = number;
        // One room a batch that left, and a read sends its next batch only once the room for it came: at most one waits.
        let (room, rooms) = unbounded();
        let sink = ResultSink::new(
            (self.core.cluster_id(), number),
            (read.max_frame, read.columns()),
            (self.pushes.clone(), rooms),
            self.core.step_count(),
        );
        let task = spawn(run_read(
            Rc::clone(&self.core),
            read,
            sink,
            self.pushes.clone(),
        ));
        let open = OpenRead {
            _task: task,
            room,
            _open: self.open.as_ref().map(OpenRequests::enter),
        };
        let _none = self.reads.insert(number, (stream, open));
        let _cancelled = self.streams.insert(stream, number);
    }

    /// The Result frame last taken from `stream` has left and the stream holds credit: the read may build its next. Nothing
    /// for a stream holding no read.
    pub fn ready(&mut self, stream: u64) {
        let held = self
            .streams
            .get(&stream)
            .and_then(|number| self.reads.get(number));
        if let Some((_, read)) = held {
            // A read that ended meanwhile wants no room.
            drop(read.room.unbounded_send(()));
        }
    }

    /// Ends the read of `stream`: its task is cancelled, each of its Scans still open with it, and no frame of it is taken
    /// from [`ReadExecutor::next`] after this call. Nothing for a stream holding no read.
    pub fn cancel(&mut self, stream: u64) {
        if let Some(number) = self.streams.remove(&stream) {
            drop(self.reads.remove(&number));
        }
    }

    /// The next frame of a read and the session's stream it is on, the frame's header naming that stream: a Result frame,
    /// or, flagged `LAST`, the read's last batch or its refusal as an Error frame. Pending while no read has a frame.
    /// A future dropped at its wait leaves the frames to come to the next call.
    pub async fn next(&mut self) -> (u64, Vec<u8>) {
        // Each pass takes one message: a frame of a read held ends it, and a read's end and a late frame, each once per read
        // and frame, are taken on (Ultra rule 2).
        loop {
            match self.pushed.next().await {
                Some(Pushed::Frame(number, mut frame)) => {
                    let Some((stream, _)) = self.reads.get(&number) else {
                        continue;
                    };
                    // A frame a read built holds its header.
                    drop(Header::stamp_stream(&mut frame, *stream));
                    return (*stream, frame);
                }
                Some(Pushed::Ended(number)) => self.ended(number),
                // The executor holds a sender, so its channel never ends while it waits here.
                None => std::future::pending::<()>().await,
            }
        }
    }

    /// The read numbered `number` ended: it is held no longer.
    fn ended(&mut self, number: u64) {
        let Some((stream, _read)) = self.reads.remove(&number) else {
            return;
        };
        if self.streams.get(&stream) == Some(&number) {
            let _ended = self.streams.remove(&stream);
        }
    }
}
