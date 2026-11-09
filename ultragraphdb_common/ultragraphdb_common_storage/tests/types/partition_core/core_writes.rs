/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the reads' and the one-frame writes' tests write and read through a core of `core_harness`: one-frame writes and
//! their answers, and the ranges of a Scan.

use std::time::Duration;

use futures::channel::mpsc::UnboundedSender;
use ultragraphdb_common_role::{Answer, RoleClient};
use ultragraphdb_common_storage::CoreMessage;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, CommitView, Credit, DirectWrite, EntriesSequence, ErrorCode,
    ErrorReplyView, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE, OperationResult, Scan, ScanAt,
    ScanRange, TermTable, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::partition_core::core_rig::{CLUSTER, DUE, PARTITION, PRINCIPAL};
use crate::types::partition_core::played_answers::{Played, Sent};

/// The next frame `client` reads, within [`DUE`].
pub(crate) async fn next_frame(client: &mut RoleClient) -> TestResult<ReceivedFrame> {
    Ok(compio::time::timeout(DUE, client.next_frame())
        .await
        .context("a frame within its time")??)
}

/// An Entries frame of a Scan read through a session: its range, through, end and entries, and whether it was the last.
#[derive(Debug, Clone, Default)]
pub(crate) struct Batch {
    pub(crate) range: u32,
    pub(crate) through: u32,
    pub(crate) end: u8,
    pub(crate) entries: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) last: bool,
}

/// Sends `scan` on `client` and reads its answer, a frame's credit granted after each frame: its Entries frames, each checked as
/// the dialling side checks it, and the code and message of the refusal ending it, if one does.
pub(crate) async fn scanned(
    client: &mut RoleClient,
    scan: &Scan,
) -> TestResult<(Vec<Batch>, Option<(ErrorCode, String)>)> {
    client.send_only(scan).await?;
    let stream = client.last_stream();
    let mut sequence = EntriesSequence::new(scan.ranges());
    let mut batches = Vec::new();
    loop {
        let frame = next_frame(client).await?;
        let view = frame.view()?;
        if view.command() == Command::Error {
            let error = ErrorReplyView::from_frame(&view)?;
            return Ok((batches, Some((error.code(), error.message().to_string()))));
        }
        let entries = sequence.accept(&view)?;
        let batch = Batch {
            range: entries.range(),
            through: entries.through(),
            end: entries.end().number(),
            entries: entries
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.key().to_vec(),
                        entry.value().unwrap_or_default().to_vec(),
                    )
                })
                .collect(),
            last: view.header().flags() & FLAG_LAST != 0,
        };
        let last = batch.last;
        batches.push(batch);
        if last {
            return Ok((batches, None));
        }
        client.send_on(stream, &Credit::new(1)).await?;
    }
}

/// The IRI `http://ex.org/<name>`.
pub(crate) fn ex(name: &str) -> String {
    format!("http://ex.org/{name}")
}

/// What a one-frame write's answer said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Committed {
    pub(crate) session: [u8; 16],
    pub(crate) request: u64,
    pub(crate) version: u64,
    pub(crate) results: Vec<OperationResult>,
}

/// The next frame on `client`: a Commit, or the code and message of its refusal.
pub(crate) async fn committed(
    client: &mut RoleClient,
) -> TestResult<Result<Committed, (ErrorCode, String)>> {
    let frame = next_frame(client).await?;
    let view = frame.view()?;
    if view.command() != Command::Commit {
        let error = ErrorReplyView::from_frame(&view)?;
        return Ok(Err((error.code(), error.message().to_string())));
    }
    let reply = CommitView::from_frame(&view)?;
    Ok(Ok(Committed {
        session: reply.session(),
        request: reply.request(),
        version: reply.commit_version(),
        results: (0..reply.result_count())
            .filter_map(|index| reply.result(index))
            .collect(),
    }))
}

/// Sends `write` on `client` and reads its answer.
pub(crate) async fn one_frame<B: BodyBuilder>(
    client: &mut RoleClient,
    write: &B,
) -> TestResult<Result<Committed, (ErrorCode, String)>> {
    client.send_only(write).await?;
    committed(client).await
}

/// A DirectWrite of `tenant` inserting each of `edges`, `(subject, predicate, object)` named by [`ex`], through no session.
pub(crate) fn edges(tenant: u64, edges: &[(&str, &str, &str)]) -> DirectWrite {
    let mut terms = TermTable::new();
    let changes = edges
        .iter()
        .map(|(subject, predicate, object)| WriteChange::Insert {
            graph: None,
            subject: terms.iri(&ex(subject)),
            predicate: terms.iri(&ex(predicate)),
            object: terms.iri(&ex(object)),
        })
        .collect();
    DirectWrite::new(tenant, terms, changes).with_principal(PRINCIPAL)
}

/// The range holding `key` alone.
pub(crate) fn alone(key: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
    let past = [key.as_slice(), &[0]].concat();
    (key, past)
}

/// A Scan of no transaction reading `ranges` at `at`, waiting `deadline_ms` for the close of `epoch`.
pub(crate) fn plain(
    at: ScanAt,
    epoch: u64,
    deadline_ms: u32,
    ranges: &[(Vec<u8>, Vec<u8>)],
) -> Scan {
    limited(at, epoch, deadline_ms, 0, ranges)
}

/// A Scan of no transaction reading at most `limit` entries of each of `ranges` at `at`, waiting `deadline_ms` for the close
/// of `epoch`.
pub(crate) fn limited(
    at: ScanAt,
    epoch: u64,
    deadline_ms: u32,
    limit: u32,
    ranges: &[(Vec<u8>, Vec<u8>)],
) -> Scan {
    let mut scan = Scan::new(at, 0, epoch, PRINCIPAL, [0; 16], limit, deadline_ms);
    for (first, past) in ranges {
        scan.push_range(ScanRange::new(PARTITION, first, past));
    }
    scan
}

/// A connection to a core that is the test itself, in place of a session: its requests go straight to the core's inbox, its answers
/// and pushed frames come back here, and the test says when a frame left, with credit or without.
#[derive(Debug)]
pub(crate) struct Direct {
    connection: u64,
    inbox: UnboundedSender<CoreMessage>,
    sequence: u128,
    played: Played,
}

impl Direct {
    /// Connection `connection` to the core whose inbox is `inbox`, its peer taking frames of at most `max_frame` bytes: a test of
    /// a frame's bounds takes small ones, so that little data takes many frames.
    pub(crate) fn open(
        inbox: &UnboundedSender<CoreMessage>,
        connection: u64,
        max_frame: u32,
    ) -> TestResult<Self> {
        Ok(Self {
            connection,
            inbox: inbox.clone(),
            sequence: 0,
            played: Played::new(1 << 20, max_frame),
        })
    }

    /// Hands `body` to the core on `stream` and reads the answer it gave on taking it, within [`DUE`].
    pub(crate) async fn request<B: BodyBuilder>(
        &mut self,
        stream: u64,
        body: &B,
    ) -> TestResult<Answer> {
        let sent = self.send(stream, body)?;
        self.played.answer(sent).await
    }

    /// Hands `body` to the core on `stream` and goes on at once: where its answer will be found.
    pub(crate) fn send<B: BodyBuilder>(&mut self, stream: u64, body: &B) -> TestResult<Sent> {
        self.sequence += 1;
        let header = Header::new(CLUSTER, B::COMMAND)
            .with_sequence(self.sequence)
            .with_stream(stream);
        let frame = body.frame(header)?;
        self.played
            .hand(&self.inbox, self.connection, stream, &frame)
    }

    /// The answer the core gave `sent` on taking it.
    pub(crate) async fn answered(&self, sent: Sent) -> TestResult<Answer> {
        self.played.answer(sent).await
    }

    /// The next frame the core pushes, and its stream, within [`DUE`].
    pub(crate) async fn pushed(&mut self) -> TestResult<(u64, Vec<u8>)> {
        self.played.frame("a pushed frame").await
    }

    /// Fails should the core push anything within `quiet`.
    pub(crate) async fn nothing_for(&mut self, quiet: Duration) -> TestResult {
        self.played.nothing_for(quiet).await
    }

    /// The frame last pushed on `stream` left, and the stream holds credit for its next.
    pub(crate) fn ready(&self, stream: u64) -> TestResult {
        Ok(self.inbox.unbounded_send(CoreMessage::Ready {
            connection: self.connection,
            stream,
        })?)
    }

    /// The frame last pushed on `stream` left, and the stream holds no credit for its next.
    pub(crate) fn left(&self, stream: u64) -> TestResult {
        Ok(self.inbox.unbounded_send(CoreMessage::Left {
            connection: self.connection,
            stream,
        })?)
    }

    /// The reader cancelled `stream`.
    pub(crate) fn cancel(&self, stream: u64) -> TestResult {
        Ok(self.inbox.unbounded_send(CoreMessage::Cancelled {
            connection: self.connection,
            stream,
        })?)
    }

    /// The connection closed.
    pub(crate) fn close(&self) -> TestResult {
        Ok(self.inbox.unbounded_send(CoreMessage::Closed {
            connection: self.connection,
        })?)
    }
}

/// `frame` viewed as the core built it.
pub(crate) fn viewed(frame: &[u8]) -> TestResult<FrameView<'_>> {
    Ok(FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?)
}

/// What `answer` refused: its code, detail and message.
pub(crate) fn refusal(answer: &Answer) -> TestResult<(ErrorCode, u64, String)> {
    match answer {
        Answer::Refuse(reply) => Ok((reply.code(), reply.detail(), reply.message().to_string())),
        Answer::Reply(_) | Answer::Open | Answer::Done => {
            Err(TestError::new(format!("a refusal, not {answer:?}")))
        }
    }
}

/// The waits of a test: `ms` milliseconds.
pub(crate) fn ms(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// Installs version `version` of the tenant's catalog through `rig`, declaring `unique` unique, and waits for its report.
pub(crate) async fn declare_unique(
    rig: &mut crate::types::partition_core::core_rig::Rig,
    version: u64,
    unique: &str,
) -> TestResult {
    let predicate =
        ultragraphdb_core_mapping::PredicateDeclaration::new(oxrdf::NamedNode::new(unique)?)
            .with_unique(true);
    let contents = ultragraphdb_core_catalog::CatalogContents::new(
        crate::types::partition_core::core_rig::TENANT,
        version,
        vec![ultragraphdb_core_mapping::Declaration::Predicate(predicate)],
    );
    rig.inbox.unbounded_send(CoreMessage::Catalog(contents))?;
    let report = rig.report().await?;
    if report.kind() != ultragraphdb_protocol_wire::ReportKind::Passed {
        return Err(TestError::new(format!(
            "the version was not passed: {report:?}"
        )));
    }
    Ok(())
}
