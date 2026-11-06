/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conformance suite against the client's core: the golden frames of `ultragraphdb_protocol_wire`, each request
//! the client sends built as its golden bytes, each reply read as its golden values, and every bit flip and truncation
//! of every golden reply refused with the connection's rule.

use std::fs;
use std::time::Instant;

use ultragraphdb_client_core::{
    BatchPoll, ClientConfig, ClientCore, ClientError, ClientErrorEnum, Pending, Reply,
    ResultStream, Transaction,
};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    Anonymous, BufferPool, Framer, ReceivedFrame, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BeginView, BodyView, CommitView, DescribeView, ErrorReplyView, ExecuteView, FrameView,
    MAX_FRAME_SIZE, OperationView, PatternSlot, PrepareView, QuadPattern, Read, ReadView,
    RegisterView, ResultView, TermTable, WriteView,
};

use crate::utils::harness::{
    CLUSTER, Harness, UNIX, begun, committed, described, frame_through, frames, prepared,
    registered, server, start,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// Golden frame `name`, read from the wire crate's `testdata/golden/bodies/`.
pub fn golden(name: &str) -> TestResult<Vec<u8>> {
    // The repository's root, three levels above this crate's `testdata/`, taken lexically: the directory need not exist.
    let testdata = ultragraphdb_tool_bdd::testdata_dir()?;
    let root = testdata
        .ancestors()
        .nth(3)
        .context("the repository's root")?;
    let path = root
        .join("ultragraphdb_protocol/ultragraphdb_protocol_wire/testdata/golden/bodies")
        .join(format!("{name}.frame"));
    fs::read(&path)
        .map_err(|e| format!("{}: {e}", path.display()))
        .context("reading the golden frame")
}

/// `frame` moved to `stream`: the header's stream rewritten and its checksum recomputed with xxhash-rust directly.
pub fn restream(frame: &[u8], stream: u64) -> Vec<u8> {
    let mut frame = frame.to_vec();
    frame[56..64].copy_from_slice(&stream.to_le_bytes());
    let checksum = xxhash_rust::xxh3::xxh3_64(&frame[8..256]);
    frame[..8].copy_from_slice(&checksum.to_le_bytes());
    frame
}

/// `frame` with the header's `sequence` of `sent`, its checksum recomputed: the client's frame numbering is tested
/// apart from the bodies.
fn resequence(frame: &[u8], sent: &[u8]) -> Vec<u8> {
    let mut frame = frame.to_vec();
    frame[16..32].copy_from_slice(&sent[16..32]);
    let checksum = xxhash_rust::xxh3::xxh3_64(&frame[8..256]);
    frame[..8].copy_from_slice(&checksum.to_le_bytes());
    frame
}

/// `frame` numbered `sequence`, its checksum recomputed.
pub fn numbered(frame: &[u8], sequence: u128) -> Vec<u8> {
    let mut frame = frame.to_vec();
    frame[16..32].copy_from_slice(&sequence.to_le_bytes());
    let checksum = xxhash_rust::xxh3::xxh3_64(&frame[8..256]);
    frame[..8].copy_from_slice(&checksum.to_le_bytes());
    frame
}

pub fn stream_of(frame: &[u8]) -> TestResult<u64> {
    Ok(u64::from_le_bytes(frame[56..64].try_into()?))
}

/// The last frame the server received.
fn last(h: &Harness) -> TestResult<Vec<u8>> {
    h.server.received().last().cloned().context("a frame")
}

pub fn view<'a, V: BodyView<'a>>(frame: &'a FrameView<'a>) -> TestResult<V::Body> {
    V::from_frame(frame)
        .and_then(|v| v.to_body())
        .context("a golden body")
}

/// The version a read-only test transaction's snapshot reads at.
const SNAPSHOT: u64 = 6;

#[test]
fn test_every_request_the_client_sends_is_its_golden_frame() -> TestResult {
    // More frames than the Welcome's credit, so the read is open for its Credit and its Cancel.
    let mut h = Harness::ready(server().with_results(9))?;
    assert_eq!(h.server.received()[0], golden("hello")?, "the Hello");
    // The golden Read and Execute name the transaction of stream 5: four sessions take streams 1 to 4.
    for _ in 0..4 {
        let p = h.core.register(h.now)?;
        assert!(matches!(h.settle(p)?, Ok(Reply::Registered(_))));
    }
    let p = h.core.begin(0, true, h.now)?;
    let reader = begun(h.settle(p)?)?;
    let beginning = h.core.begin(0, false, h.now)?;
    let writer = begun(h.settle(beginning)?)?;
    let mut sent = Vec::new();
    let mut check = |harness: &mut Harness, name: &str, stream: u64| -> TestResult {
        while harness.step()? {}
        let frame = last(harness)?;
        assert_eq!(
            frame,
            resequence(&restream(&golden(name)?, stream), &frame),
            "{name}"
        );
        sent.push(name.to_string());
        Ok(())
    };
    for name in ["write", "operation", "operation_counter", "operation_deep"] {
        let g = golden(name)?;
        let frame = FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE)?;
        if name == "write" {
            h.core.write(&writer, &view::<WriteView>(&frame)?)?;
        } else {
            h.core.operation(&writer, &view::<OperationView>(&frame)?)?;
        }
        check(&mut h, name, writer.stream())?;
    }
    h.core.abort(writer)?;
    check(&mut h, "abort", writer.stream())?;
    let g = golden("read")?;
    let stream = h.core.read(
        &reader,
        &view::<ReadView>(&FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE)?)?,
        h.now,
    )?;
    assert_eq!(reader.stream(), 5);
    check(&mut h, "read", stream.stream())?;
    h.core.grant(&stream, frames(2)?, h.now)?;
    check(&mut h, "credit", stream.stream())?;
    h.core.cancel(stream);
    check(&mut h, "cancel", stream.stream())?;
    prepare_describe_execute(&mut h, &reader)?;
    assert_eq!(sent.len(), 8);
    Ok(())
}

/// The Prepare, the Describe and an Execute of `reader`, each sent as its golden frame.
fn prepare_describe_execute(h: &mut Harness, reader: &Transaction) -> TestResult {
    let g = golden("prepare")?;
    let p = h.core.prepare(
        &view::<PrepareView>(&FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE)?)?,
        h.now,
    )?;
    while h
        .core
        .transmit()
        .map(|f| h.server.receive(&f, h.now))
        .is_some()
    {}
    let prepared = last(h)?;
    assert_eq!(
        prepared,
        resequence(&restream(&g, stream_of(&prepared)?), &prepared),
        "prepare"
    );
    let _prepared: Result<Reply, ClientError> = h.settle(p)?;
    let describing = h.core.describe(0, h.now)?;
    while h
        .core
        .transmit()
        .map(|f| h.server.receive(&f, h.now))
        .is_some()
    {}
    let described = last(h)?;
    assert_eq!(
        described,
        resequence(
            &restream(&golden("describe")?, stream_of(&described)?),
            &described
        ),
        "describe"
    );
    let _described: Result<Reply, ClientError> = h.settle(describing)?;
    let execute = golden("execute")?;
    let _stream: ResultStream = h.core.execute(
        reader,
        &view::<ExecuteView>(&FrameView::validate(&execute, CLUSTER, MAX_FRAME_SIZE)?)?,
        h.now,
    )?;
    while h
        .core
        .transmit()
        .map(|f| h.server.receive(&f, h.now))
        .is_some()
    {}
    let executed = last(h)?;
    assert_eq!(
        executed,
        resequence(&restream(&execute, stream_of(&executed)?), &executed),
        "execute"
    );
    Ok(())
}

/// A ready core of the golden frames' cluster and release, with no server, and its connection's framer: frames are
/// handed to it directly, each numbered as the connection's next.
struct Direct {
    core: ClientCore<Anonymous>,
    framer: Framer,
    sent: u128,
}

impl Direct {
    fn new(pool: &BufferPool) -> TestResult<Self> {
        let config = ClientConfig::new(CLUSTER, "ultragraph-test")
            .with_release(crate::utils::harness::RELEASE);
        let mut direct = Self {
            core: ClientCore::new(config, Anonymous),
            framer: Framer::new(CLUSTER, MAX_FRAME_SIZE),
            sent: 0,
        };
        direct.core.connect(start(), UNIX)?;
        while direct.core.transmit().is_some() {}
        direct.deliver(pool, &golden("welcome_no_auth")?)??;
        Ok(direct)
    }

    /// `frame` numbered as the connection's next.
    fn next(&mut self, frame: &[u8]) -> Vec<u8> {
        self.sent += 1;
        numbered(frame, self.sent)
    }

    /// `bytes` as they arrive, through the connection's framer.
    fn frame(&mut self, pool: &BufferPool, bytes: &[u8]) -> Result<ReceivedFrame, TransportError> {
        frame_through(&mut self.framer, pool, bytes)
    }

    /// `frame` numbered as the connection's next and received: the core's answer, or the test's failure when `frame`
    /// is no whole frame.
    fn deliver(&mut self, pool: &BufferPool, frame: &[u8]) -> TestResult<Result<(), ClientError>> {
        let stamped = self.next(frame);
        let received = self.frame(pool, &stamped).context("a whole frame")?;
        Ok(self.core.receive(received, start()))
    }
}

#[test]
fn test_every_golden_reply_is_read_as_its_values() -> TestResult {
    let pool = BufferPool::new(8, 1 << 20);
    let now = start();
    let mut d = Direct::new(&pool)?;
    let w = d.core.welcome().context("a Welcome")?;
    assert_eq!(
        (
            w.protocol(),
            w.max_frame(),
            w.max_streams(),
            w.initial_credit(),
            w.server()
        ),
        (1, 1 << 19, 64, 4, "ultragraph 0.1")
    );
    assert!(
        !w.authenticates(),
        "the golden Welcome of a server asking for none, its max_frame at the floor"
    );
    let answer = |direct: &mut Direct,
                  pending: Pending,
                  name: &str,
                  stream: u64|
     -> TestResult<Result<Reply, ClientError>> {
        direct.deliver(&pool, &restream(&golden(name)?, stream))??;
        direct.core.take_reply(pending).context("an answer")
    };
    let p = d.core.register(now)?;
    let session = registered(answer(&mut d, p, "register", 1)?)?;
    let register_bytes = golden("register")?;
    let register = view::<RegisterView>(&FrameView::validate(
        &register_bytes,
        CLUSTER,
        MAX_FRAME_SIZE,
    )?)?;
    assert_eq!(session.number(), u128::from_be_bytes(register.session()));
    let beginning = d.core.begin(0, true, now)?;
    let tx = begun(answer(&mut d, beginning, "begin", 2)?)?;
    let begin_bytes = golden("begin")?;
    let begin = view::<BeginView>(&FrameView::validate(&begin_bytes, CLUSTER, MAX_FRAME_SIZE)?)?;
    assert_eq!(
        (tx.tenant(), tx.read_version(), tx.read_only()),
        (begin.tenant(), begin.read_version(), begin.read_only())
    );
    let preparing = d.core.prepare(
        &view::<PrepareView>(&FrameView::validate(
            &golden("prepare")?,
            CLUSTER,
            MAX_FRAME_SIZE,
        )?)?,
        now,
    )?;
    let statement = prepared(answer(&mut d, preparing, "prepare_reply", 3)?)?;
    let reply_bytes = golden("prepare_reply")?;
    let reply = view::<PrepareView>(&FrameView::validate(&reply_bytes, CLUSTER, MAX_FRAME_SIZE)?)?;
    assert_eq!(
        (statement.id(), statement.schema(), statement.result_types()),
        (reply.statement(), reply.schema(), reply.result_types())
    );
    let describing = d.core.describe(0, now)?;
    let describe = described(answer(&mut d, describing, "describe_reply", 4)?)?;
    let description = golden("describe_reply")?;
    let expected =
        view::<DescribeView>(&FrameView::validate(&description, CLUSTER, MAX_FRAME_SIZE)?)?;
    assert_eq!(describe, expected);
    Ok(())
}

/// A transaction begun on `stream` of `d`, a Begin reply of tenant 0 delivered: a read-only one at its snapshot's
/// version `SNAPSHOT`, a read-write one at 0, as `PROTOCOL.md` §6.2 answers them.
fn begin_direct(
    d: &mut Direct,
    pool: &BufferPool,
    now: Instant,
    stream: u64,
    read_only: bool,
) -> TestResult<Transaction> {
    let p = d.core.begin(0, read_only, now)?;
    let frame = ultragraphdb_protocol_wire::BodyBuilder::frame(
        &ultragraphdb_protocol_wire::Begin::new(0, if read_only { SNAPSHOT } else { 0 }, read_only),
        ultragraphdb_protocol_wire::Header::new(
            CLUSTER,
            ultragraphdb_protocol_wire::Command::Begin,
        )
        .with_stream(stream),
    )?;
    d.deliver(pool, &frame)??;
    begun(d.core.take_reply(p).context("an answer")?)
}

#[test]
fn test_golden_commits_results_and_errors_are_read_as_their_values() -> TestResult {
    let pool = BufferPool::new(8, 1 << 20);
    let now = start();
    let mut d = Direct::new(&pool)?;
    // The session and request the golden Commit answers, resumed so the client sends exactly that commit.
    let reply_bytes = golden("commit")?;
    let reply = view::<CommitView>(&FrameView::validate(&reply_bytes, CLUSTER, MAX_FRAME_SIZE)?)?;
    let session = d
        .core
        .resume_session(u128::from_be_bytes(reply.session()), reply.request())?;
    let tx = begin_direct(&mut d, &pool, now, 1, false)?;
    let p = d.core.commit(tx, session, now)?;
    d.deliver(&pool, &restream(&golden("commit")?, 1))??;
    let committed = committed(d.core.take_reply(p).context("an answer")?)?;
    assert_eq!(
        (committed.version(), committed.request()),
        (reply.commit_version(), reply.request())
    );
    assert_eq!(committed.results(), reply.results());
    assert_eq!(d.core.next_request(session), Some(reply.request() + 1));
    golden_result_is_read(&mut d, &pool, now)?;
    golden_errors_are_read(&mut d, &pool, now)?;
    Ok(())
}

/// The golden Result, on the third stream of `d`, read as its values.
/// Every cell of a batch of `columns` and `rows`, column by column, as `cell` gives it.
fn every_cell(columns: u32, rows: u32, cell: impl Fn(u32, u32) -> Option<u32>) -> Vec<Option<u32>> {
    (0..columns)
        .flat_map(|col| (0..rows).map(move |row| (col, row)))
        .map(|(col, row)| cell(col, row))
        .collect()
}

fn golden_result_is_read(d: &mut Direct, pool: &BufferPool, now: Instant) -> TestResult {
    let reader = begin_direct(d, pool, now, 2, true)?;
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let read = Read::new(
        reader.stream(),
        0,
        terms,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(a),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        ),
        Vec::new(),
    );
    let stream = d.core.read(&reader, &read, now)?;
    assert_eq!(stream.stream(), 3, "a stream of its own");
    d.deliver(pool, &restream(&golden("result")?, 3))??;
    // What the batch must deliver, read from the golden Result itself by the wire crate's view.
    let golden_bytes = golden("result")?;
    let golden_frame = FrameView::validate(&golden_bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let golden_view = ResultView::from_frame(&golden_frame)?;
    let expected = (
        golden_view.columns(),
        golden_view.rows(),
        every_cell(golden_view.columns(), golden_view.rows(), |c, r| {
            golden_view.cell(c, r)
        }),
        golden_view.resume().to_vec(),
    );
    let polled = d.core.next_batch(&stream, now, |v| {
        let cells = every_cell(v.columns(), v.rows(), |c, r| v.cell(c, r));
        (v.columns(), v.rows(), cells, v.resume().to_vec())
    });
    assert!(matches!(polled, Ok(BatchPoll::Batch(_))), "a batch");
    let Ok(BatchPoll::Batch((columns, rows, cells, resume))) = polled else {
        return Ok(());
    };
    assert_eq!((columns, rows, cells, resume), expected);
    assert!(
        matches!(d.core.next_batch(&stream, now, |_| ()), Ok(BatchPoll::Done)),
        "the golden Result is flagged last"
    );
    Ok(())
}

/// The golden Errors, on the fourth and fifth streams of `d`, read as their refusals.
fn golden_errors_are_read(d: &mut Direct, pool: &BufferPool, now: Instant) -> TestResult {
    for (name, stream) in [("error", 4), ("error_operation", 5)] {
        // The code, detail and value the golden Error names, read from the frame.
        let bytes = golden(name)?;
        let reply = view::<ErrorReplyView>(&FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE)?)?;
        let (code, detail, value) = (reply.code(), reply.detail(), reply.value().cloned());
        let tx = begin_direct(d, pool, now, stream, false)?;
        let session = d.core.resume_session(u128::from(100 + stream), 1)?;
        let p = d.core.commit(tx, session, now)?;
        d.deliver(pool, &restream(&golden(name)?, stream))??;
        let error = d
            .core
            .take_reply(p)
            .context("an answer")?
            .err()
            .context("an error")?;
        assert!(
            matches!(&error.0, ClientErrorEnum::Refused { code: found_code, detail: found_detail, value: found_value, .. } if *found_code == code && *found_detail == detail && *found_value == value),
            "{name}: {error:?}"
        );
        assert_eq!(error.retryable(), code.retryable());
    }
    Ok(())
}

/// Every bit flip and every truncation of every golden reply is refused: by the framer (header) or by the core with
/// the error the wire's own validation gives, never read as a value.
#[test]
fn test_every_damaged_golden_reply_is_refused_with_the_connections_rule() -> TestResult {
    let pool = BufferPool::new(8, 1 << 20);
    let now = start();
    let mut cases = 0;
    for name in [
        "register",
        "begin",
        "commit",
        "prepare_reply",
        "describe_reply",
        "result",
        "error",
        "error_operation",
    ] {
        let good = numbered(&restream(&golden(name)?, 1), 2);
        for at in 0..good.len() {
            let mut bad = good.clone();
            bad[at] ^= 1 << (at % 8);
            cases += 1;
            let mut d = Direct::new(&pool)?;
            let p = waiting_for(&mut d, name, now)?;
            let frame = match d.frame(&pool, &bad) {
                Ok(frame) => frame,
                Err(error) => {
                    assert!(
                        matches!(error.0, TransportErrorEnum::Protocol { .. }),
                        "{name} at {at}: {error:?}"
                    );
                    continue;
                }
            };
            let expected = refusal_of(name, &bad)?;
            match d.core.receive(frame, now) {
                Err(error) => {
                    if let Some(rule) = expected {
                        assert_eq!(
                            error.0,
                            ClientErrorEnum::Protocol { error: rule },
                            "{name} at {at}"
                        );
                    }
                    assert!(!d.core.is_ready());
                }
                Ok(()) => {
                    assert!(expected.is_none(), "{name} at {at}: a damaged frame passed");
                    assert!(
                        d.core.take_reply(p).is_none_or(|r| r.is_err()),
                        "{name} at {at}: read as a value"
                    );
                }
            }
        }
        truncations_are_refused(&pool, name, &good)?;
    }
    assert!(cases > 2_000, "{cases}");
    Ok(())
}

/// The request the golden reply `name` answers, on the connection's first stream; a Begin for any other.
fn waiting_for(d: &mut Direct, name: &str, now: Instant) -> TestResult<Pending> {
    Ok(match name {
        "register" => d.core.register(now)?,
        "prepare_reply" => d.core.prepare(
            &view::<PrepareView>(&FrameView::validate(
                &golden("prepare")?,
                CLUSTER,
                MAX_FRAME_SIZE,
            )?)?,
            now,
        )?,
        "describe_reply" => d.core.describe(0, now)?,
        _ => d.core.begin(0, true, now)?,
    })
}

/// The rule a damaged golden reply `name` breaks, if any: a Begin waits on the stream, so a Commit or a Result is no
/// answer of it, whatever its body holds, and is refused at step 3, before its chunks.
fn refusal_of(name: &str, bad: &[u8]) -> TestResult<Option<ProtocolError>> {
    if ["commit", "result"].contains(&name) {
        let command = ultragraphdb_protocol_wire::HeaderView::new(bad)?.command();
        return Ok(Some(ProtocolError::command_not_served(command)));
    }
    Ok(FrameView::validate(bad, CLUSTER, MAX_FRAME_SIZE).err())
}

/// Every truncation of golden reply `good`, named `name`, refused by the framer.
fn truncations_are_refused(pool: &BufferPool, name: &str, good: &[u8]) -> TestResult {
    for len in 0..good.len() {
        let mut d = Direct::new(pool)?;
        // Nothing read is a close between frames; a cut header is short of its 256 bytes, a cut body of the
        // frame's size.
        let expected = match len {
            0 => TransportError::closed(),
            len if len < 256 => TransportError::truncated(len, 256),
            len => TransportError::truncated(len, good.len()),
        };
        assert_eq!(
            d.frame(pool, &good[..len]).err(),
            Some(expected),
            "{name} cut at {len}"
        );
    }
    Ok(())
}
