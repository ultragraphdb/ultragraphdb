/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A connection a test plays in place of a session, and the answer the core gives each request when it takes it. The core pushes
//! every answer, in its order, so a test learns a request's answer by following the request with a probe the core refuses at once,
//! an Abort on a stream no Bind opened: the core takes both in one turn, so whatever it pushes on the request's stream before the
//! probe's refusal is the answer it gave on taking the request, and nothing there is [`Answer::Open`]. What else the connection
//! reads meanwhile is kept, in order, for its later reads. The tests run on one thread and no borrow here crosses a wait.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::{Answer, ParkedBytes, Push};
use ultragraphdb_common_storage::{CoreMessage, ParkedFrame};
use ultragraphdb_protocol_wire::{
    Abort, BodyBuilder, BodyView, Command, ErrorReply, ErrorReplyView, FLAG_LAST, FrameView,
    Header, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::types::partition_core::core_rig::{CLUSTER, DUE};

/// The first stream a probe takes: far above any a test numbers itself.
const PROBES_FROM: u64 = 1 << 62;
/// How often a read looks for the next push.
const LOOK: Duration = Duration::from_millis(1);

/// A request handed to the core, followed by its probe: where its answer will be found.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sent {
    stream: u64,
    probe: u64,
}

/// A connection's side of what the core pushes it.
#[derive(Debug)]
pub(crate) struct Played {
    held: ParkedBytes,
    pushes: UnboundedSender<(u64, Push)>,
    pushed: RefCell<UnboundedReceiver<(u64, Push)>>,
    kept: RefCell<VecDeque<(u64, Push)>>,
    probes: Cell<u64>,
    max_frame: u32,
}

impl Played {
    /// A connection holding at most `bound` bytes of requests before it would stop reading, its peer taking frames of at most
    /// `max_frame` bytes.
    pub(crate) fn new(bound: usize, max_frame: u32) -> Self {
        let (pushes, pushed) = unbounded();
        Self {
            held: ParkedBytes::new(bound),
            pushes,
            pushed: RefCell::new(pushed),
            kept: RefCell::new(VecDeque::new()),
            probes: Cell::new(PROBES_FROM),
            max_frame,
        }
    }

    /// What it holds now: its requests not yet taken by the core and those parked.
    pub(crate) fn held(&self) -> usize {
        self.held.count()
    }

    /// `frame`, a request of `connection` on `stream`, handed to the core as a session hands it, and then a probe: where its answer
    /// will be found.
    pub(crate) fn hand(
        &self,
        core: &UnboundedSender<CoreMessage>,
        connection: u64,
        stream: u64,
        frame: &[u8],
    ) -> TestResult<Sent> {
        core.unbounded_send(self.message(connection, stream, frame)?)?;
        let probe = self.probes.get();
        self.probes.set(probe + 1);
        let abort = Abort.frame(Header::new(CLUSTER, Command::Abort).with_stream(probe))?;
        core.unbounded_send(self.message(connection, probe, &abort)?)?;
        Ok(Sent { stream, probe })
    }

    /// `frame` as the request message a session sends its core, copied and counted in what the connection holds.
    fn message(&self, connection: u64, stream: u64, frame: &[u8]) -> TestResult<CoreMessage> {
        Ok(CoreMessage::Request {
            connection,
            stream,
            frame: ParkedFrame::copy(frame, self.held.clone()),
            pushes: self.pushes.clone(),
            max_frame: self.max_frame,
        })
    }

    /// The answer the core gave `sent` on taking it, within [`DUE`].
    pub(crate) async fn answer(&self, sent: Sent) -> TestResult<Answer> {
        let mut answer = None;
        let mut passed = VecDeque::new();
        loop {
            let (stream, push) = self.next_push("a probe's refusal").await?;
            if stream == sent.probe {
                break;
            }
            if stream == sent.stream && answer.is_none() {
                answer = Some(push);
            } else {
                passed.push_back((stream, push));
            }
        }
        // What came for other streams goes back ahead of what is still kept, in its order.
        let mut kept = self.kept.borrow_mut();
        while let Some(push) = passed.pop_back() {
            kept.push_front(push);
        }
        drop(kept);
        answer.map_or(Ok(Answer::Open), answered)
    }

    /// The next push, kept or newly come, within [`DUE`], or a failure naming `what` it waited for.
    async fn next_push(&self, what: &str) -> TestResult<(u64, Push)> {
        let mut waited = Duration::ZERO;
        loop {
            if let Some(push) = self.take() {
                return Ok(push);
            }
            if waited >= DUE {
                return Err(TestError::new(format!(
                    "nothing pushed within {DUE:?}: {what}"
                )));
            }
            compio::time::sleep(LOOK).await;
            waited += LOOK;
        }
    }

    /// A push kept, or one come since, without waiting.
    fn take(&self) -> Option<(u64, Push)> {
        if let Some(push) = self.kept.borrow_mut().pop_front() {
            return Some(push);
        }
        self.pushed.borrow_mut().try_recv().ok()
    }

    /// The next frame pushed, and its stream, or a failure naming `what` it waited for; a stream's end is no frame.
    pub(crate) async fn frame(&self, what: &str) -> TestResult<(u64, Vec<u8>)> {
        match self.next_push(what).await? {
            (stream, Push::Frame(frame)) => Ok((stream, frame)),
            (stream, Push::End) => Err(TestError::new(format!(
                "stream {stream} ended with no frame: {what}"
            ))),
            (stream, Push::Refuse(refusal)) => Ok((stream, refused(stream, &refusal)?)),
        }
    }

    /// Whether anything was pushed and waits to be read now.
    pub(crate) fn holds_push(&self) -> bool {
        match self.take() {
            Some(push) => {
                self.kept.borrow_mut().push_front(push);
                true
            }
            None => false,
        }
    }

    /// Fails should anything be pushed within `quiet`.
    pub(crate) async fn nothing_for(&self, quiet: Duration) -> TestResult {
        let mut waited = Duration::ZERO;
        while waited < quiet {
            if let Some((stream, push)) = self.take() {
                return Err(TestError::new(format!(
                    "pushed within {quiet:?} on stream {stream}: {push:?}"
                )));
            }
            compio::time::sleep(LOOK).await;
            waited += LOOK;
        }
        Ok(())
    }
}

/// `refusal` on `stream` as the Error frame a connection sends for it.
fn refused(stream: u64, refusal: &ErrorReply) -> TestResult<Vec<u8>> {
    Ok(refusal.frame(
        Header::new(CLUSTER, Command::Error)
            .with_stream(stream)
            .with_flags(FLAG_LAST),
    )?)
}

/// The answer `push` gives: a reply, a refusal, an Error frame as a refusal, or a stream's end as [`Answer::Done`].
fn answered(push: Push) -> TestResult<Answer> {
    match push {
        Push::End => Ok(Answer::Done),
        Push::Refuse(refusal) => Ok(Answer::Refuse(refusal)),
        Push::Frame(frame) => {
            let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
            if view.command() != Command::Error {
                return Ok(Answer::Reply(frame));
            }
            let error = ErrorReplyView::from_frame(&view)?;
            let mut refusal =
                ErrorReply::new(error.code(), error.message()).with_detail(error.detail());
            if let Some(value) = error.value() {
                refusal = refusal.with_value(value.to_term());
            }
            Ok(Answer::Refuse(refusal))
        }
    }
}
