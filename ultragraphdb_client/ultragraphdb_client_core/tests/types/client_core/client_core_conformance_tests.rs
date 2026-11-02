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
use ultragraphdb_protocol_transport::{
    Anonymous, BufferPool, Framer, ReceivedFrame, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BodyView, ColumnType, DescribeView, ErrorCode, ExecuteView, FrameView, MAX_FRAME_SIZE,
    OperationResult, OperationView, PatternSlot, PrepareView, QuadPattern, Read, ReadView,
    TermTable, WriteView,
};

use crate::utils::harness::{
    CLUSTER, Harness, UNIX, begun, committed, described, frame_through, prepared, registered,
    server, start,
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

#[test]
fn test_every_request_the_client_sends_is_its_golden_frame() -> TestResult {
    let mut h = Harness::ready(server())?;
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
    h.core.grant(&stream, 2)?;
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
    assert_eq!(session.number(), 0x1122_3344_5566_7788);
    let beginning = d.core.begin(0, true, now)?;
    let tx = begun(answer(&mut d, beginning, "begin", 2)?)?;
    assert_eq!(
        (tx.namespace(), tx.read_version(), tx.read_only()),
        (3, 0xAB_CDEF_0002, true)
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
    assert_eq!(
        (statement.id(), statement.schema(), statement.result_types()),
        (1, 0x5C4E_3A2B_1D0F_7E6A, &[ColumnType::Term][..])
    );
    let describing = d.core.describe(0, now)?;
    let describe = described(answer(&mut d, describing, "describe_reply", 4)?)?;
    let reply = golden("describe_reply")?;
    let expected = view::<DescribeView>(&FrameView::validate(&reply, CLUSTER, MAX_FRAME_SIZE)?)?;
    assert_eq!((describe.read_version(), &describe), (42, &expected));
    Ok(())
}

/// A transaction begun on `stream` of `d`, its golden Begin reply of namespace 0 and read version 5 delivered.
fn begin_direct(
    d: &mut Direct,
    pool: &BufferPool,
    now: Instant,
    stream: u64,
    read_only: bool,
) -> TestResult<Transaction> {
    let p = d.core.begin(0, read_only, now)?;
    let frame = ultragraphdb_protocol_wire::BodyBuilder::frame(
        &ultragraphdb_protocol_wire::Begin::new(0, 5, read_only),
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
    let session = d.core.resume_session(9, 7);
    let tx = begin_direct(&mut d, &pool, now, 1, false)?;
    let p = d.core.commit(tx, session, now)?;
    d.deliver(&pool, &restream(&golden("commit")?, 1))??;
    let committed = committed(d.core.take_reply(p).context("an answer")?)?;
    assert_eq!((committed.version(), committed.request()), (1_000, 7));
    assert_eq!(
        committed.results(),
        &[
            OperationResult::new(0x10, true),
            OperationResult::new(0x20, false)
        ]
    );
    assert_eq!(d.core.next_request(session), Some(8));
    golden_result_is_read(&mut d, &pool, now)?;
    golden_errors_are_read(&mut d, &pool, now)?;
    Ok(())
}

/// The golden Result, on the third stream of `d`, read as its values.
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
    let polled = d.core.next_batch(&stream, now, |v| {
        let cells: Vec<Option<u32>> = (0..2)
            .flat_map(|col| (0..3).map(move |row| (col, row)))
            .map(|(col, row)| v.cell(col, row))
            .collect();
        (v.columns(), v.rows(), cells, v.resume().to_vec())
    });
    assert!(matches!(polled, Ok(BatchPoll::Batch(_))), "a batch");
    let Ok(BatchPoll::Batch((columns, rows, cells, resume))) = polled else {
        return Ok(());
    };
    assert_eq!((columns, rows, resume), (2, 3, b"rk".to_vec()));
    assert_eq!(
        cells,
        vec![Some(0), Some(0), Some(3), Some(1), Some(2), Some(u32::MAX)]
    );
    assert!(
        matches!(d.core.next_batch(&stream, now, |_| ()), Ok(BatchPoll::Done)),
        "the golden Result is flagged last"
    );
    Ok(())
}

/// The golden Errors, on the fourth and fifth streams of `d`, read as their refusals.
fn golden_errors_are_read(d: &mut Direct, pool: &BufferPool, now: Instant) -> TestResult {
    for (name, stream, code, detail, value) in [
        ("error", 4, ErrorCode::Conflict, 3, None),
        (
            "error_operation",
            5,
            ErrorCode::OperationFailed,
            0,
            Some(ultragraphdb_protocol_wire::RequestTerm::Literal {
                lexical: "done".into(),
                datatype: None,
            }),
        ),
    ] {
        let tx = begin_direct(d, pool, now, stream, false)?;
        let session = d.core.resume_session(100 + stream, 1);
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
            let p = match name {
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
            };
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
            let expected = FrameView::validate(&bad, CLUSTER, MAX_FRAME_SIZE).err();
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
