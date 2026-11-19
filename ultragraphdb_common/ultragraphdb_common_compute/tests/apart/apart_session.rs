/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client's reads as its session hands them to [`ReadExecutor`]: each Read built by the wire's builder and read back by its
//! view, taken with what the session holds of the read's transaction as the read starts (`PROTOCOL.md` §11.12, *Reads*), and
//! its frames taken as the client's connection lets them go, the read given room for its next batch while its stream holds
//! credit (§11.3).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::pin::pin;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::future::{Either, select};
use ultragraphdb_common_compute::{ReadExecutor, ReadPattern, ReadRequest};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorReply, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE,
    QuadPattern, Read, ReadLevel, ReadView, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// The stream of the Begin every Read names: the session takes the read's transaction from its own state.
const TRANSACTION_STREAM: u64 = 1;

/// A client's Read, by its pattern's terms, its limit and its resume key, and what its session takes of the read's
/// transaction as the read starts: the tenant, the level, the partition, the identity, the version and epoch, the
/// principal and the deadline; with the client's `max_frame` and the credit its stream starts with.
#[derive(Debug, Clone)]
pub struct SessionRead {
    tenant: u64,
    read_only: bool,
    level: ReadLevel,
    terms: TermTable,
    pattern: QuadPattern,
    resume: Vec<u8>,
    limit: u32,
    partition: u32,
    transaction: [u8; 16],
    read_version: u64,
    named: bool,
    epoch: u64,
    principal: u64,
    deadline_ms: u32,
    max_frame: u32,
    credit: u32,
}

impl SessionRead {
    /// A read of `pattern`, naming `terms`, from `resume`, in a transaction of `tenant` at `level`, read-only or read-write;
    /// every other field 0 until set.
    pub fn new(
        tenant: u64,
        read_only: bool,
        level: ReadLevel,
        terms: TermTable,
        pattern: QuadPattern,
        resume: Vec<u8>,
    ) -> Self {
        Self {
            tenant,
            read_only,
            level,
            terms,
            pattern,
            resume,
            limit: 0,
            partition: 0,
            transaction: [0; 16],
            read_version: 0,
            named: false,
            epoch: 0,
            principal: 0,
            deadline_ms: 0,
            max_frame: 0,
            credit: 0,
        }
    }

    /// The transaction's partition.
    #[must_use]
    pub fn with_partition(mut self, partition: u32) -> Self {
        self.partition = partition;
        self
    }

    /// A read-write transaction's identity, as its Bind named it.
    #[must_use]
    pub fn with_transaction(mut self, transaction: [u8; 16]) -> Self {
        self.transaction = transaction;
        self
    }

    /// The version the read reads at: the Bind's answer, or the snapshot's version bound.
    #[must_use]
    pub fn with_read_version(mut self, read_version: u64) -> Self {
        self.read_version = read_version;
        self.named = false;
        self
    }

    /// The version the client named, which the read reads at without waiting.
    #[must_use]
    pub fn with_named_version(mut self, read_version: u64) -> Self {
        self.read_version = read_version;
        self.named = true;
        self
    }

    /// The epoch the read waits for: the snapshot's, the token's, or the read's floor.
    #[must_use]
    pub fn with_epoch(mut self, epoch: u64) -> Self {
        self.epoch = epoch;
        self
    }

    /// The client's principal.
    #[must_use]
    pub fn with_principal(mut self, principal: u64) -> Self {
        self.principal = principal;
        self
    }

    /// The most rows the Read asks for; 0 for no limit.
    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// The milliseconds left until the transaction's deadline.
    #[must_use]
    pub fn with_deadline_ms(mut self, deadline_ms: u32) -> Self {
        self.deadline_ms = deadline_ms;
        self
    }

    /// The `max_frame` of the client's Hello and the credit the read's stream starts with.
    #[must_use]
    pub fn with_client_limits(mut self, max_frame: u32, credit: u32) -> Self {
        self.max_frame = max_frame;
        self.credit = credit;
        self
    }

    /// The version the read reads at; 0 for none.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The epoch the read waits for; 0 for none.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The client's Read frame on `stream` of a connection of `cluster`.
    pub fn frame(&self, cluster: u64, stream: u64) -> TestResult<Vec<u8>> {
        let read = Read::new(
            TRANSACTION_STREAM,
            self.limit,
            self.terms.clone(),
            self.pattern,
            self.resume.clone(),
        );
        Ok(read.frame(Header::new(cluster, Command::Read).with_stream(stream))?)
    }

    /// The pattern hash a resume key of the read holds: its Read's, over the pattern's sections as the frame carries them.
    pub fn pattern_hash(&self, cluster: u64) -> TestResult<u64> {
        let frame = self.frame(cluster, TRANSACTION_STREAM)?;
        let view = FrameView::validate(&frame, cluster, MAX_FRAME_SIZE)?;
        Ok(ReadView::from_frame(&view)?.pattern_hash())
    }

    /// The read as the session hands it over, from its Read on `stream` of a connection of `cluster`; the refusal the
    /// session answers the Read with where its pattern is refused.
    fn request(&self, cluster: u64, stream: u64) -> TestResult<Result<ReadRequest, ErrorReply>> {
        let frame = self.frame(cluster, stream)?;
        let view = FrameView::validate(&frame, cluster, MAX_FRAME_SIZE)?;
        let pattern = ReadView::from_frame(&view)
            .map_err(|rule| ErrorReply::protocol(&rule))
            .and_then(|read| ReadPattern::of_read(&read));
        Ok(pattern.map(|pattern| {
            let read = ReadRequest::new(pattern, self.tenant, self.read_only, self.level)
                .with_partition(self.partition)
                .with_transaction(self.transaction)
                .with_epoch(self.epoch)
                .with_principal(self.principal)
                .with_deadline_ms(self.deadline_ms)
                .with_max_frame(self.max_frame);
            if self.named {
                read.with_named_version(self.read_version)
            } else {
                read.with_read_version(self.read_version)
            }
        }))
    }
}

/// A frame the client's connection let go.
#[derive(Debug)]
pub struct SessionFrame {
    cluster: u64,
    bytes: Vec<u8>,
}

impl SessionFrame {
    /// The frame's bytes, header and body.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The frame validated for its connection.
    pub fn view(&self) -> TestResult<FrameView<'_>> {
        Ok(FrameView::validate(
            &self.bytes,
            self.cluster,
            MAX_FRAME_SIZE,
        )?)
    }
}

/// What the session asks of its executor.
#[derive(Debug)]
enum Ask {
    Start(u64, Box<ReadRequest>),
    Ready(u64),
    Cancel(u64),
}

/// What the executor's task next takes: a frame the executor handed out, or the session's next ask, `None` once the
/// session is gone.
enum Turn {
    Frame((u64, Vec<u8>)),
    Asked(Option<Ask>),
}

/// Runs `executor` for as long as the session asks of it, as a connection's serve loop does: its frames taken as it hands
/// them out, so a read's end is taken as it comes, and passed on to `frames`; the executor, and every read it holds,
/// dropped with the session.
async fn run_executor(
    mut executor: ReadExecutor,
    mut asks: UnboundedReceiver<Ask>,
    frames: UnboundedSender<(u64, Vec<u8>)>,
) {
    // Each pass takes one frame or one ask; the session's end ends it.
    loop {
        let turn = {
            let next = pin!(executor.next());
            match select(next, asks.next()).await {
                Either::Left((frame, _)) => Turn::Frame(frame),
                Either::Right((ask, _)) => Turn::Asked(ask),
            }
        };
        match turn {
            Turn::Frame(frame) => {
                if frames.unbounded_send(frame).is_err() {
                    return;
                }
            }
            Turn::Asked(Some(Ask::Start(stream, read))) => executor.start(stream, *read),
            Turn::Asked(Some(Ask::Ready(stream))) => executor.ready(stream),
            Turn::Asked(Some(Ask::Cancel(stream))) => executor.cancel(stream),
            Turn::Asked(None) => return,
        }
    }
}

/// One client connection's reads, run by a [`ReadExecutor`] on a task of its own as the connection's session runs them: each
/// on a stream of its own, a Result frame spending a frame of its stream's credit as the client takes it, and the read given
/// room for its next while credit remains, a Credit giving room to a read that waited for it. Dropped, it drops its reads, as
/// a connection's end does.
#[derive(Debug)]
pub struct ReadSession {
    cluster: u64,
    asks: UnboundedSender<Ask>,
    frames: UnboundedReceiver<(u64, Vec<u8>)>,
    last: u64,
    credit: BTreeMap<u64, u32>,
    waiting: BTreeSet<u64>,
    refused: VecDeque<Vec<u8>>,
}

impl ReadSession {
    /// The reads of a connection of `cluster`, run by `executor` on a task of this runtime.
    pub fn new(cluster: u64, executor: ReadExecutor) -> Self {
        let (asks, asked) = unbounded();
        let (handed, frames) = unbounded();
        spawn(run_executor(executor, asked, handed)).detach();
        Self {
            cluster,
            asks,
            frames,
            last: 0,
            credit: BTreeMap::new(),
            waiting: BTreeSet::new(),
            refused: VecDeque::new(),
        }
    }

    /// Starts `read` on a stream of its own; a read whose pattern the session refuses is answered by its refusal alone.
    pub fn send_only(&mut self, read: &SessionRead) -> TestResult {
        self.last += 1;
        let stream = self.last;
        match read.request(self.cluster, stream)? {
            Ok(request) => {
                let _none = self.credit.insert(stream, read.credit);
                self.ask(Ask::Start(stream, Box::new(request)))?;
            }
            Err(refusal) => {
                let header = Header::new(self.cluster, Command::Error)
                    .with_stream(stream)
                    .with_flags(FLAG_LAST);
                self.refused.push_back(refusal.frame(header)?);
            }
        }
        Ok(())
    }

    /// The stream of the read started last.
    pub fn last_stream(&self) -> u64 {
        self.last
    }

    /// Grants `stream` credit for `frames` more Result frames; a read that waited for credit has room for its next.
    pub fn credit(&mut self, stream: u64, frames: u32) -> TestResult {
        if let Some(held) = self.credit.get_mut(&stream) {
            *held = held.saturating_add(frames);
            if *held > 0 && self.waiting.remove(&stream) {
                self.ask(Ask::Ready(stream))?;
            }
        }
        Ok(())
    }

    /// Cancels the read on `stream`.
    pub fn cancel(&mut self, stream: u64) -> TestResult {
        let _held = self.credit.remove(&stream);
        let _waited = self.waiting.remove(&stream);
        self.ask(Ask::Cancel(stream))
    }

    /// The next frame the connection lets go: a refusal of a read's pattern first, then the executor's frames in the order
    /// it hands them out. Dropped at its wait, it loses nothing.
    pub async fn next_frame(&mut self) -> TestResult<SessionFrame> {
        if let Some(bytes) = self.refused.pop_front() {
            return Ok(self.framed(bytes));
        }
        let (stream, bytes) = self
            .frames
            .next()
            .await
            .context("the executor's task ended")?;
        let (last, result) = {
            let view = FrameView::validate(&bytes, self.cluster, MAX_FRAME_SIZE)?;
            (
                view.header().flags() & FLAG_LAST != 0,
                view.command() == Command::Result,
            )
        };
        if last {
            let _held = self.credit.remove(&stream);
            let _waited = self.waiting.remove(&stream);
        } else if result {
            self.left(stream)?;
        }
        Ok(self.framed(bytes))
    }

    /// A Result frame of `stream`, not its last, has left: it spent a frame of credit, and the read has room for its next
    /// while credit remains.
    fn left(&mut self, stream: u64) -> TestResult {
        let Some(held) = self.credit.get_mut(&stream) else {
            return Ok(());
        };
        *held = held.saturating_sub(1);
        if *held > 0 {
            self.ask(Ask::Ready(stream))
        } else {
            let _new = self.waiting.insert(stream);
            Ok(())
        }
    }

    /// Hands `ask` to the executor's task.
    fn ask(&self, ask: Ask) -> TestResult {
        self.asks
            .unbounded_send(ask)
            .map_err(|_| TestError::new("the executor's task ended"))
    }

    /// `bytes` as a frame of the connection.
    fn framed(&self, bytes: Vec<u8>) -> SessionFrame {
        SessionFrame {
            cluster: self.cluster,
            bytes,
        }
    }
}
