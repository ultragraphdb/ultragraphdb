/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Reads driven through [`ReadExecutor`] as a client's session drives it: the client's Read built by the wire's builder and
//! read back by its view, handed over as a [`ReadRequest`], and its frames taken with room given after each but the last, as
//! a connection with credit to spare gives it.

use std::time::Duration;

use ultragraphdb_common_compute::{ReadExecutor, ReadPattern, ReadRequest};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorReplyView, FLAG_LAST, FrameView, Header, HeaderView,
    MAX_FRAME_SIZE, MAX_FRAME_SIZE_U32, PatternSlot, QuadPattern, Read, ReadLevel, ReadView,
    ResultView, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::apart_cluster::CLUSTER;
use super::apart_partition::{EPOCH, VERSION};
use super::apart_reads::{ANSWER_WAIT, ReadAnswer, TENANT, TRANSACTION};

/// The client's stream every read here runs on.
pub const STREAM: u64 = 5;

/// A client's Read frame of a pattern whose object is unbound, the graph the default one unless `any_graph`, of at most
/// `limit` rows from `resume`.
pub fn read_frame(
    (any_graph, subject, predicate): (bool, Option<&str>, Option<&str>),
    limit: u32,
    resume: Vec<u8>,
) -> TestResult<Vec<u8>> {
    let mut terms = TermTable::new();
    let graph = if any_graph {
        PatternSlot::Unbound
    } else {
        PatternSlot::DefaultGraph
    };
    let subject = subject.map_or(PatternSlot::Unbound, |iri| {
        PatternSlot::Bound(terms.iri(iri))
    });
    let predicate = predicate.map_or(PatternSlot::Unbound, |iri| {
        PatternSlot::Bound(terms.iri(iri))
    });
    let pattern = QuadPattern::new(graph, subject, predicate, PatternSlot::Unbound);
    let header = Header::new(CLUSTER, Command::Read).with_stream(STREAM);
    Ok(Read::new(1, limit, terms, pattern, resume).frame(header)?)
}

/// What the Read `frame` asks.
pub fn pattern_of(frame: &[u8]) -> TestResult<ReadPattern> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    ReadPattern::of_read(&ReadView::from_frame(&view)?)
        .map_err(|refusal| TestError::new(format!("{refusal:?}")))
}

/// `pattern`'s read in the read-write transaction bound at partition 1, as `apart_reads::read_of` names it.
pub fn bound(pattern: ReadPattern) -> ReadRequest {
    ReadRequest::new(pattern, TENANT, false, ReadLevel::Snapshot)
        .with_partition(1)
        .with_transaction(TRANSACTION)
        .with_read_version(VERSION)
        .with_principal(1)
        .with_deadline_ms(10_000)
        .with_max_frame(MAX_FRAME_SIZE_U32)
}

/// `pattern`'s read in a read-only transaction at snapshot level, at the played partitions' epoch, its batches of at most
/// `max_frame` bytes.
pub fn snapshot(pattern: ReadPattern, max_frame: u32) -> ReadRequest {
    ReadRequest::new(pattern, TENANT, true, ReadLevel::Snapshot)
        .with_read_version(VERSION)
        .with_epoch(EPOCH)
        .with_principal(1)
        .with_deadline_ms(10_000)
        .with_max_frame(max_frame)
}

/// The next frame `executor` hands out, and its stream, failing past [`ANSWER_WAIT`].
pub async fn next_of(executor: &mut ReadExecutor) -> TestResult<(u64, Vec<u8>)> {
    compio::time::timeout(ANSWER_WAIT, executor.next())
        .await
        .map_err(|_| TestError::new(format!("the executor gave no frame within {ANSWER_WAIT:?}")))
}

/// Whether `executor` hands out a frame within `wait`.
pub async fn gives_within(executor: &mut ReadExecutor, wait: Duration) -> bool {
    compio::time::timeout(wait, executor.next()).await.is_ok()
}

/// Adds `frame`, a Result or an Error frame, to `answer`: whether it was the stream's last.
pub fn take(answer: &mut ReadAnswer, frame: &[u8]) -> TestResult<bool> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    if view.command() == Command::Error {
        let error = ErrorReplyView::from_frame(&view)?;
        answer.refused = Some((error.code(), error.detail(), error.message().to_string()));
        return Ok(true);
    }
    let result = ResultView::from_frame(&view)?;
    for row in 0..result.rows() {
        let cells = (0..result.columns())
            .map(|column| {
                let cell = result.cell(column, row).context("a cell")?;
                Ok(result
                    .term(cell)
                    .and_then(|term| term.text().map(str::to_string)))
            })
            .collect::<TestResult<Vec<_>>>()?;
        answer.rows.push(cells);
    }
    answer.resume = result.resume().to_vec();
    answer.batches += 1;
    Ok(view.header().flags() & FLAG_LAST != 0)
}

/// Runs `read` on `executor` as [`STREAM`]'s to its end, room given after every frame but the last: its answer, and the size of
/// every frame it gave.
pub async fn executed(
    executor: &mut ReadExecutor,
    read: ReadRequest,
) -> TestResult<(ReadAnswer, Vec<usize>)> {
    executor.start(STREAM, read);
    let mut answer = ReadAnswer::default();
    let mut sizes = Vec::new();
    // Each pass takes one frame of a read that ends with its last, within the test's own wait (Ultra rule 2).
    loop {
        let (stream, frame) = next_of(executor).await?;
        assert_eq!(stream, STREAM, "a frame of another stream");
        assert_eq!(
            HeaderView::new(&frame)?.stream(),
            STREAM,
            "the frame names its stream"
        );
        sizes.push(frame.len());
        if take(&mut answer, &frame)? {
            return Ok((answer, sizes));
        }
        executor.ready(STREAM);
    }
}
