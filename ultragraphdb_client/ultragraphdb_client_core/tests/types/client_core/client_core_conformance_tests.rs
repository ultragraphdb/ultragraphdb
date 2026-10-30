/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conformance suite against the client's core: the golden frames of `ultragraphdb_protocol_wire`, each request
//! the client sends built as its golden bytes, each reply read as its golden values, and every bit flip and truncation
//! of every golden reply refused with the connection's rule.

use std::fs;

use ultragraphdb_client_core::{
    BatchPoll, ClientConfig, ClientCore, ClientError, ClientErrorEnum, Pending, Reply,
};
use ultragraphdb_protocol_transport::{
    Anonymous, BufferPool, Framer, ReceivedFrame, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BodyView, ColumnType, DescribeView, ErrorCode, ExecuteView, FrameView, MAX_FRAME_SIZE,
    OperationResult, OperationView, PatternSlot, PrepareView, QuadPattern, Read, ReadView,
    TermTable, WriteView,
};

use crate::utils::harness::{CLUSTER, Harness, UNIX, frame_through, server, start};

/// Golden frame `name`, read from the wire crate's `testdata/golden/bodies/`.
fn golden(name: &str) -> Vec<u8> {
    // The repository's root, three levels above this crate's `testdata/`, taken lexically: the directory need not exist.
    let testdata = ultragraphdb_tool_bdd::testdata_dir();
    let root = testdata.ancestors().nth(3).expect("the repository's root");
    let path = root
        .join("ultragraphdb_protocol/ultragraphdb_protocol_wire/testdata/golden/bodies")
        .join(format!("{name}.frame"));
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `frame` moved to `stream`: the header's stream rewritten and its checksum recomputed with xxhash-rust directly.
fn restream(frame: &[u8], stream: u64) -> Vec<u8> {
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
fn numbered(frame: &[u8], sequence: u128) -> Vec<u8> {
    let mut frame = frame.to_vec();
    frame[16..32].copy_from_slice(&sequence.to_le_bytes());
    let checksum = xxhash_rust::xxh3::xxh3_64(&frame[8..256]);
    frame[..8].copy_from_slice(&checksum.to_le_bytes());
    frame
}

fn stream_of(frame: &[u8]) -> u64 {
    u64::from_le_bytes(frame[56..64].try_into().unwrap())
}

/// The last frame the server received.
fn last(h: &Harness) -> Vec<u8> {
    h.server.received().last().cloned().expect("a frame")
}

fn view<'a, V: BodyView<'a>>(frame: &'a FrameView<'a>) -> V::Body {
    V::from_frame(frame)
        .and_then(|v| v.to_body())
        .expect("a golden body")
}

#[test]
fn test_every_request_the_client_sends_is_its_golden_frame() {
    let mut h = Harness::ready(server());
    assert_eq!(h.server.received()[0], golden("hello"), "the Hello");
    // The golden Read and Execute name the transaction of stream 5: four sessions take streams 1 to 4.
    for _ in 0..4 {
        let p = h.core.register(h.now).unwrap();
        assert!(matches!(h.settle(p), Ok(Reply::Registered(_))));
    }
    let p = h.core.begin(0, true, h.now).unwrap();
    let Ok(Reply::Begun(reader)) = h.settle(p) else {
        panic!()
    };
    let p = h.core.begin(0, false, h.now).unwrap();
    let Ok(Reply::Begun(writer)) = h.settle(p) else {
        panic!()
    };
    let mut sent = Vec::new();
    let mut check = |h: &mut Harness, name: &str, stream: u64| {
        while h.step() {}
        let frame = last(h);
        assert_eq!(
            frame,
            resequence(&restream(&golden(name), stream), &frame),
            "{name}"
        );
        sent.push(name.to_string());
    };
    for name in ["write", "operation", "operation_counter", "operation_deep"] {
        let g = golden(name);
        let frame = FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE).unwrap();
        if name == "write" {
            h.core.write(&writer, &view::<WriteView>(&frame)).unwrap();
        } else {
            h.core
                .operation(&writer, &view::<OperationView>(&frame))
                .unwrap();
        }
        check(&mut h, name, writer.stream());
    }
    h.core.abort(writer).unwrap();
    check(&mut h, "abort", writer.stream());
    let g = golden("read");
    let stream = h
        .core
        .read(
            &reader,
            &view::<ReadView>(&FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE).unwrap()),
            h.now,
        )
        .unwrap();
    assert_eq!(reader.stream(), 5);
    check(&mut h, "read", stream.stream());
    h.core.grant(&stream, 2).unwrap();
    check(&mut h, "credit", stream.stream());
    h.core.cancel(stream);
    check(&mut h, "cancel", stream.stream());
    let g = golden("prepare");
    let p = h
        .core
        .prepare(
            &view::<PrepareView>(&FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE).unwrap()),
            h.now,
        )
        .unwrap();
    while h
        .core
        .transmit()
        .map(|f| h.server.receive(&f, h.now))
        .is_some()
    {}
    let prepared = last(&h);
    assert_eq!(
        prepared,
        resequence(&restream(&g, stream_of(&prepared)), &prepared),
        "prepare"
    );
    let _ = h.settle(p);
    let p = h.core.describe(0, h.now).unwrap();
    while h
        .core
        .transmit()
        .map(|f| h.server.receive(&f, h.now))
        .is_some()
    {}
    let described = last(&h);
    assert_eq!(
        described,
        resequence(
            &restream(&golden("describe"), stream_of(&described)),
            &described
        ),
        "describe"
    );
    let _ = h.settle(p);
    let g = golden("execute");
    let _ = h
        .core
        .execute(
            &reader,
            &view::<ExecuteView>(&FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE).unwrap()),
            h.now,
        )
        .unwrap();
    while h
        .core
        .transmit()
        .map(|f| h.server.receive(&f, h.now))
        .is_some()
    {}
    let executed = last(&h);
    assert_eq!(
        executed,
        resequence(&restream(&g, stream_of(&executed)), &executed),
        "execute"
    );
    assert_eq!(sent.len(), 8);
}

/// A ready core of the golden frames' cluster and release, with no server, and its connection's framer: frames are
/// handed to it directly, each numbered as the connection's next.
struct Direct {
    core: ClientCore<Anonymous>,
    framer: Framer,
    sent: u128,
}

impl Direct {
    fn new(pool: &BufferPool) -> Self {
        let config = ClientConfig::new(CLUSTER, "ultragraph-test")
            .with_release(crate::utils::harness::RELEASE);
        let mut direct = Self {
            core: ClientCore::new(config, Anonymous),
            framer: Framer::new(CLUSTER, MAX_FRAME_SIZE),
            sent: 0,
        };
        direct.core.connect(start(), UNIX).unwrap();
        while direct.core.transmit().is_some() {}
        direct.deliver(pool, &golden("welcome_no_auth")).unwrap();
        direct
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

    /// `frame` numbered as the connection's next and received.
    fn deliver(&mut self, pool: &BufferPool, frame: &[u8]) -> Result<(), ClientError> {
        let frame = self.next(frame);
        let received = self.frame(pool, &frame).expect("a whole frame");
        self.core.receive(received, start())
    }
}

#[test]
fn test_every_golden_reply_is_read_as_its_values() {
    let pool = BufferPool::new(8, 1 << 20);
    let now = start();
    let mut d = Direct::new(&pool);
    let w = d.core.welcome().unwrap();
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
    let answer = |d: &mut Direct, p: Pending, name: &str, stream: u64| {
        d.deliver(&pool, &restream(&golden(name), stream)).unwrap();
        d.core.take_reply(p).expect("an answer")
    };
    let p = d.core.register(now).unwrap();
    let Ok(Reply::Registered(session)) = answer(&mut d, p, "register", 1) else {
        panic!()
    };
    assert_eq!(session.number(), 0x1122_3344_5566_7788);
    let p = d.core.begin(0, true, now).unwrap();
    let Ok(Reply::Begun(tx)) = answer(&mut d, p, "begin", 2) else {
        panic!()
    };
    assert_eq!(
        (tx.namespace(), tx.read_version(), tx.read_only()),
        (3, 0xAB_CDEF_0002, true)
    );
    let p = d
        .core
        .prepare(
            &view::<PrepareView>(
                &FrameView::validate(&golden("prepare"), CLUSTER, MAX_FRAME_SIZE).unwrap(),
            ),
            now,
        )
        .unwrap();
    let Ok(Reply::Prepared(statement)) = answer(&mut d, p, "prepare_reply", 3) else {
        panic!()
    };
    assert_eq!(
        (statement.id(), statement.schema(), statement.result_types()),
        (1, 0x5C4E_3A2B_1D0F_7E6A, &[ColumnType::Term][..])
    );
    let p = d.core.describe(0, now).unwrap();
    let Ok(Reply::Described(describe)) = answer(&mut d, p, "describe_reply", 4) else {
        panic!()
    };
    let reply = golden("describe_reply");
    let expected =
        view::<DescribeView>(&FrameView::validate(&reply, CLUSTER, MAX_FRAME_SIZE).unwrap());
    assert_eq!((describe.read_version(), &describe), (42, &expected));
}

#[test]
fn test_golden_commits_results_and_errors_are_read_as_their_values() {
    let pool = BufferPool::new(8, 1 << 20);
    let now = start();
    let mut d = Direct::new(&pool);
    let begin = |d: &mut Direct, stream: u64, read_only: bool| {
        let p = d.core.begin(0, read_only, now).unwrap();
        let frame = ultragraphdb_protocol_wire::BodyBuilder::frame(
            &ultragraphdb_protocol_wire::Begin::new(0, 5, read_only),
            ultragraphdb_protocol_wire::Header::new(
                CLUSTER,
                ultragraphdb_protocol_wire::Command::Begin,
            )
            .with_stream(stream),
        )
        .unwrap();
        d.deliver(&pool, &frame).unwrap();
        let Some(Ok(Reply::Begun(tx))) = d.core.take_reply(p) else {
            panic!()
        };
        tx
    };
    let session = d.core.resume_session(9, 7);
    let tx = begin(&mut d, 1, false);
    let p = d.core.commit(tx, session, now).unwrap();
    d.deliver(&pool, &restream(&golden("commit"), 1)).unwrap();
    let Some(Ok(Reply::Committed(committed))) = d.core.take_reply(p) else {
        panic!()
    };
    assert_eq!((committed.version(), committed.request()), (1_000, 7));
    assert_eq!(
        committed.results(),
        &[
            OperationResult::new(0x10, true),
            OperationResult::new(0x20, false)
        ]
    );
    assert_eq!(d.core.next_request(session), Some(8));
    let reader = begin(&mut d, 2, true);
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
    let stream = d.core.read(&reader, &read, now).unwrap();
    assert_eq!(stream.stream(), 3, "a stream of its own");
    d.deliver(&pool, &restream(&golden("result"), 3)).unwrap();
    let Ok(BatchPoll::Batch((columns, rows, cells, resume))) =
        d.core.next_batch(&stream, now, |v| {
            let cells: Vec<Option<u32>> = (0..2)
                .flat_map(|col| (0..3).map(move |row| (col, row)))
                .map(|(col, row)| v.cell(col, row))
                .collect();
            (v.columns(), v.rows(), cells, v.resume().to_vec())
        })
    else {
        panic!()
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
        let tx = begin(&mut d, stream, false);
        let session = d.core.resume_session(100 + stream, 1);
        let p = d.core.commit(tx, session, now).unwrap();
        d.deliver(&pool, &restream(&golden(name), stream)).unwrap();
        let error = d.core.take_reply(p).unwrap().unwrap_err();
        assert!(
            matches!(&error.0, ClientErrorEnum::Refused { code: c, detail: d, value: v, .. } if *c == code && *d == detail && *v == value),
            "{name}: {error:?}"
        );
        assert_eq!(error.retryable(), code.retryable());
    }
}

/// Every bit flip and every truncation of every golden reply is refused: by the framer (header) or by the core with
/// the error the wire's own validation gives, never read as a value.
#[test]
fn test_every_damaged_golden_reply_is_refused_with_the_connections_rule() {
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
        let good = numbered(&restream(&golden(name), 1), 2);
        for at in 0..good.len() {
            let mut bad = good.clone();
            bad[at] ^= 1 << (at % 8);
            cases += 1;
            let mut d = Direct::new(&pool);
            let p = match name {
                "register" => d.core.register(now).unwrap(),
                "prepare_reply" => d
                    .core
                    .prepare(
                        &view::<PrepareView>(
                            &FrameView::validate(&golden("prepare"), CLUSTER, MAX_FRAME_SIZE)
                                .unwrap(),
                        ),
                        now,
                    )
                    .unwrap(),
                "describe_reply" => d.core.describe(0, now).unwrap(),
                _ => d.core.begin(0, true, now).unwrap(),
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
        for len in 0..good.len() {
            let mut d = Direct::new(&pool);
            // Nothing read is a close between frames; a cut header is short of its 256 bytes, a cut body of the
            // frame's size.
            let expected = match len {
                0 => TransportError::Closed(),
                len if len < 256 => TransportError::Truncated(len, 256),
                len => TransportError::Truncated(len, good.len()),
            };
            match d.frame(&pool, &good[..len]) {
                Err(error) => assert_eq!(error, expected, "{name} cut at {len}"),
                Ok(_) => panic!("{name} cut at {len} read whole"),
            }
        }
    }
    assert!(cases > 2_000, "{cases}");
}
