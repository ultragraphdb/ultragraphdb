/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::channel::oneshot;
use ultragraphdb_common_role::{RoleClient, RoleReader, RoleWriter};
use ultragraphdb_protocol_transport::{BufferPool, ReceivedFrame};
use ultragraphdb_protocol_wire::{BodyBuilder, ErrorReply, Header, HeaderView, MAX_FRAME_SIZE};

use crate::utils::clock::now;
use crate::utils::refusals::unavailable;

/// A link's frame buffers at a time: the reader goes on past this one from a new pool while a frame is held, so a read that does
/// not consume holds up no other and an idle link keeps one buffer resident.
const LINK_BUFFERS: usize = 1;

/// One core's connection to one partition, every read of the core sharing it: each stream's frames reach that stream's holder,
/// and a frame compute refuses, or a partition silent past [`Link::answer_wait`], closes the connection, ending every stream with it
/// (`PROTOCOL.md` §11.12, §13.14).
#[derive(Debug)]
pub(crate) struct Link {
    partition: u32,
    cluster: u64,
    /// How long the partition may leave a request unanswered past its deadline before compute takes it for hung.
    answer_wait: Duration,
    out: UnboundedSender<Vec<u8>>,
    streams: RefCell<HashMap<u64, UnboundedSender<ReceivedFrame>>>,
    next_stream: Cell<u64>,
    closed: Cell<bool>,
    /// Ends the reader at a close, so the connection's read half, and with it the socket, goes with the link.
    closing: RefCell<Option<oneshot::Sender<()>>>,
    /// When the partition last sent a frame on the connection.
    heard: Cell<Instant>,
}

impl Link {
    /// The link of `partition` over the authenticated `client`, its reader and writer spawned on this core.
    pub(crate) fn start(
        partition: u32,
        cluster: u64,
        answer_wait: Duration,
        client: RoleClient,
    ) -> Rc<Self> {
        let (reader, writer, _welcome) = client.split();
        let (out, sending) = unbounded();
        let (closing, closed) = oneshot::channel();
        let link = Rc::new(Self {
            partition,
            cluster,
            answer_wait,
            out,
            streams: RefCell::default(),
            next_stream: Cell::new(1),
            closed: Cell::new(false),
            closing: RefCell::new(Some(closing)),
            heard: Cell::new(now()),
        });
        spawn(write_frames(writer, sending)).detach();
        spawn(read_frames(reader, Rc::clone(&link), closed)).detach();
        link
    }

    /// The partition.
    pub(crate) fn partition(&self) -> u32 {
        self.partition
    }

    /// How long the partition may leave a request unanswered past its deadline before compute takes it for hung.
    pub(crate) fn answer_wait(&self) -> Duration {
        self.answer_wait
    }

    /// Whether the connection has closed.
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.get()
    }

    /// A new stream and what its frames arrive through.
    pub(crate) fn open_stream(
        &self,
    ) -> Result<(u64, UnboundedReceiver<ReceivedFrame>), ErrorReply> {
        if self.closed.get() {
            return Err(self.closed_under_read());
        }
        let stream = self.next_stream.get();
        self.next_stream.set(stream.saturating_add(1));
        let (frames, arriving) = unbounded();
        let _new = self.streams.borrow_mut().insert(stream, frames);
        Ok((stream, arriving))
    }

    /// Sends `body` on `stream`.
    pub(crate) fn send<B: BodyBuilder>(&self, stream: u64, body: &B) -> Result<(), ErrorReply> {
        let frame = body
            .frame(Header::new(self.cluster, B::COMMAND).with_stream(stream))
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        self.out
            .unbounded_send(frame)
            .map_err(|_| self.closed_under_read())
    }

    /// `stream` no longer wants its frames.
    pub(crate) fn forget(&self, stream: u64) {
        drop(self.streams.borrow_mut().remove(&stream));
    }

    /// The Error a read takes for the connection closing under it.
    pub(crate) fn closed_under_read(&self) -> ErrorReply {
        unavailable(format!(
            "the connection to partition {} closed under the read",
            self.partition
        ))
    }

    /// Closes the connection, for `why`: every stream on it ends, and its reader with it.
    pub(crate) fn close(&self, why: &dyn std::fmt::Display) {
        if !self.closed.replace(true) {
            tracing::warn!(partition = self.partition, %why, "a partition's connection closes");
        }
        self.out.close_channel();
        self.streams.borrow_mut().clear();
        if let Some(closing) = self.closing.borrow_mut().take() {
            // A reader gone has nothing left to end.
            let _ended = closing.send(());
        }
    }

    /// Watches a request asked at `asked` and given up before its answer, as a read cancelled while it waits: a connection that
    /// has sent no frame by `asked` and `wait` is hung, as the request's own wait would have found, and closes
    /// (`PROTOCOL.md` §11.12, *Errors*).
    pub(crate) fn watch(self: &Rc<Self>, asked: Instant, wait: Duration) {
        let link = Rc::clone(self);
        spawn(async move {
            let due = asked.checked_add(wait).unwrap_or(asked);
            compio::time::sleep(due.saturating_duration_since(now())).await;
            if !link.is_closed() && link.heard.get() < asked {
                let why = format!(
                    "partition {} sent no frame within {wait:?} of a request given up",
                    link.partition
                );
                link.close(&why);
            }
        })
        .detach();
    }

    /// Hands `frame` to its stream's holder; a frame of a stream gone had no one to go to.
    fn deliver(&self, stream: u64, frame: ReceivedFrame) {
        if let Some(holder) = self.streams.borrow().get(&stream) {
            // A holder gone is a read that ended meanwhile.
            drop(holder.unbounded_send(frame));
        }
    }
}

/// Writes each frame `sending` holds, in order, until the link closes, then shuts the connection's sending side.
async fn write_frames(mut writer: RoleWriter, mut sending: UnboundedReceiver<Vec<u8>>) {
    while let Some(frame) = sending.next().await {
        if writer.send(frame).await.is_err() {
            break;
        }
    }
    writer.shutdown().await;
}

/// Reads every frame of the connection and hands each to its stream's holder, until the connection closes, a frame breaks its
/// place on it, or the link closes (`closed`). A frame's holder takes its turn before the reader takes a new pool, so a read
/// consuming as frames come reuses the one buffer.
async fn read_frames(mut reader: RoleReader, link: Rc<Link>, mut closed: oneshot::Receiver<()>) {
    let mut pool = BufferPool::new(LINK_BUFFERS, MAX_FRAME_SIZE);
    loop {
        if pool.available() == 0 {
            yield_once().await;
        }
        if pool.available() == 0 {
            pool = BufferPool::new(LINK_BUFFERS, MAX_FRAME_SIZE);
        }
        let read = std::pin::pin!(reader.next_frame_in(&pool));
        let frame = match futures::future::select(read, &mut closed).await {
            futures::future::Either::Left((Ok(frame), _)) => frame,
            futures::future::Either::Left((Err(error), _)) => return link.close(&error),
            // The link closed: its reader ends, and the connection with it.
            futures::future::Either::Right(_) => return,
        };
        link.heard.set(now());
        if let Err(rule) = frame.check_order() {
            return link.close(&rule);
        }
        let stream = HeaderView::new(frame.bytes()).map_or(0, |header| header.stream());
        if stream == 0 {
            return link.close(&"the partition ended the connection");
        }
        link.deliver(stream, frame);
    }
}

/// Lets every other task of the core run once, waiting on nothing.
async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            return std::task::Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await;
}
