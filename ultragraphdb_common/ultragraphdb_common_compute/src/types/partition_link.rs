/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compio::runtime::spawn;
use futures::channel::mpsc::UnboundedReceiver;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{BodyBuilder, ErrorReply};

use crate::types::role_link::RoleLink;
use crate::utils::clock::now;
use crate::utils::refusals::unavailable;
use crate::utils::yield_once::yield_once;

/// A read's way to one partition: the core's one connection to the partition's role address, which its client sessions' Bind
/// streams share (design D2), each read's Scans and Closed a stream of it whose frames reach that read whole. A frame compute
/// refuses, or a partition silent past [`PartitionLink::answer_wait`], closes the connection, ending every stream on it, the
/// sessions' among them (`PROTOCOL.md` §11.12, §13.14).
///
/// Shared by an `Rc` among the reads of its core, on the core's one thread, the step count with the core (Ultra rule 9).
#[derive(Debug)]
pub(crate) struct PartitionLink {
    link: RoleLink,
    partition: u32,
    /// How long the partition may leave a request unanswered past its deadline before compute takes it for hung.
    answer_wait: Duration,
    /// The core's count of its reads' steps.
    steps: Rc<Cell<u64>>,
}

impl PartitionLink {
    /// `partition`'s way over `link`, a partition silent past `answer_wait` hung, each frame a read takes from it a step counted
    /// in `steps`.
    pub(crate) fn new(
        link: RoleLink,
        partition: u32,
        (answer_wait, steps): (Duration, Rc<Cell<u64>>),
    ) -> Self {
        Self {
            link,
            partition,
            answer_wait,
            steps,
        }
    }

    /// Ends a step on this core, which consumed a frame of this link: the step is counted when it is a read's, and the core given
    /// back, so every other ready task of the core runs before the next step (design D3).
    /// A dropped future leaves the step counted.
    pub(crate) async fn step(&self, counted: bool) {
        if counted {
            self.steps.set(self.steps.get().saturating_add(1));
        }
        yield_once().await;
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
        self.link.is_closed()
    }

    /// Opens a stream by the request `body`, a Scan or a Closed: its number and what its frames arrive through.
    ///
    /// # Errors
    ///
    /// The rule the request's frame breaks; code 17 for a connection closed under the read.
    pub(crate) fn open<B: BodyBuilder>(
        &self,
        body: &B,
    ) -> Result<(u64, UnboundedReceiver<ReceivedFrame>), ErrorReply> {
        if self.link.is_closed() {
            return Err(self.closed_under_read());
        }
        self.link.open_read(body)
    }

    /// Grants `stream` one more Entries frame.
    ///
    /// # Errors
    ///
    /// Code 17 for a connection closed under the read.
    pub(crate) fn grant(&self, stream: u64) -> Result<(), ErrorReply> {
        if self.link.is_closed() {
            return Err(self.closed_under_read());
        }
        self.link.grant(stream);
        Ok(())
    }

    /// `stream` no longer wants its frames, and nothing at the partition is left to cancel.
    pub(crate) fn forget(&self, stream: u64) {
        let _unopened = self.link.forget(stream);
    }

    /// `stream` no longer wants its frames: it is cancelled at the partition.
    pub(crate) fn cancel(&self, stream: u64) {
        self.link.cancel(stream);
    }

    /// The Error a read takes for the connection closing under it.
    pub(crate) fn closed_under_read(&self) -> ErrorReply {
        unavailable(format!(
            "the connection to partition {} closed under the read",
            self.partition
        ))
    }

    /// Closes the connection, for `why`: every stream on it ends. Its first close logs why.
    pub(crate) fn close(&self, why: &(impl std::fmt::Display + ?Sized)) {
        if !self.link.is_closed() {
            tracing::warn!(partition = self.partition, %why, "a partition's connection closes");
        }
        self.link.close();
    }

    /// Watches a request asked at `asked` and given up before its answer, as a read cancelled while it waits: a connection that
    /// has sent no frame by `asked` and `wait` is hung, as the request's own wait would have found, and closes
    /// (`PROTOCOL.md` §11.12, *Errors*). One task per request given up, each ending by `asked` and `wait`, so the reads
    /// given up within one answer bound bound them (Ultra rule 2).
    pub(crate) fn watch(self: &Rc<Self>, asked: Instant, wait: Duration) {
        let link = Rc::clone(self);
        spawn(async move {
            let due = asked.checked_add(wait).unwrap_or(asked);
            compio::time::sleep(due.saturating_duration_since(now())).await;
            if !link.is_closed() && link.link.heard() < asked {
                let why = format!(
                    "partition {} sent no frame within {wait:?} of a request given up",
                    link.partition
                );
                link.close(&why);
            }
        })
        .detach();
    }
}
