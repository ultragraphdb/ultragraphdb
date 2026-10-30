/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{
    BatchPoll, ClientErrorEnum, Reply, ResultStream, Session, TestServer, Transaction,
};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_transport::{Anonymous, NetworkFaults};
use ultragraphdb_protocol_wire::{
    ABSENT, BodyView, ColumnType, Command, CreditView, ErrorCode, Execute, FrameView, Header,
    HeaderView, MAX_FRAME_SIZE, PatternSlot, Prepare, QuadPattern, Read, ReadView, TermTable,
    Welcome,
};

use crate::utils::harness::{CLUSTER, Harness, RELEASE, config, server};

fn begin(h: &mut Harness) -> Transaction {
    let p = h.core.begin(0, true, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!("a transaction")
    };
    tx
}

fn begin_writing(h: &mut Harness) -> (Session, Transaction) {
    let p = h.core.register(h.now).unwrap();
    let Ok(Reply::Registered(session)) = h.settle(p) else {
        panic!("a session")
    };
    let p = h.core.begin(0, false, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!("a transaction")
    };
    (session, tx)
}

/// A read of `tx`, naming it.
fn read(tx: &Transaction) -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Read::new(
        tx.stream(),
        0,
        terms,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(a),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        ),
        Vec::new(),
    )
}

/// Every batch of `stream`, each read in place as its rows' first cell's text, and the Credit frames sent.
fn drain(h: &mut Harness, stream: &ResultStream) -> Vec<Vec<String>> {
    let mut batches = Vec::new();
    let mut done = false;
    h.until(|h| {
        loop {
            match h.core.next_batch(stream, h.now, |view| {
                (0..view.rows())
                    .map(|row| {
                        let cell = view.cell(0, row).unwrap_or(ABSENT);
                        assert_eq!(
                            view.term_count(),
                            1,
                            "a repeated term is sent once per batch"
                        );
                        view.term(cell)
                            .and_then(|t| t.text())
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect::<Vec<_>>()
            }) {
                Ok(BatchPoll::Batch(rows)) => batches.push(rows),
                Ok(BatchPoll::Done) => {
                    done = true;
                    return true;
                }
                Ok(BatchPoll::Wait) => return false,
                Err(e) => panic!("{e:?}"),
            }
        }
    });
    assert!(done);
    batches
}

fn credits(h: &Harness) -> Vec<u32> {
    h.server
        .received()
        .iter()
        .filter_map(|f| {
            FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE)
                .ok()
                .and_then(|v| CreditView::from_frame(&v).ok().map(|c| c.frames()))
        })
        .collect()
}

#[test]
fn test_a_read_streams_its_batches_read_in_place_until_the_last() {
    let mut h = Harness::ready(server().with_results(5));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    let batches = drain(&mut h, &stream);
    assert_eq!(batches, vec![vec!["http://ex.org/same".to_string(); 2]; 5]);
    assert_eq!(
        credits(&h),
        vec![2],
        "4 frames of initial credit, then one grant of the configured 2"
    );
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Done)
    ));
}

/// Credit stops a stream, from the client's side: with no initial credit the client grants its window first, and the
/// server sends that many frames before it waits.
#[test]
fn test_without_credit_the_client_grants_before_any_frame_arrives() {
    let welcome = Welcome::new(1, RELEASE, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(8)
        .with_initial_credit(0);
    let mut h = Harness::with(
        config(),
        TestServer::new(CLUSTER, welcome).with_results(3),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect();
    h.until(|h| h.core.is_ready());
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Wait)
    ));
    assert_eq!(drain(&mut h, &stream).len(), 3);
    assert_eq!(credits(&h), vec![2, 2]);
    assert_eq!(
        h.core.grant(&stream, 1).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        },
        "a finished read takes no credit"
    );
}

#[test]
fn test_cancel_ends_the_stream_and_its_later_frames_are_dropped() {
    let mut h = Harness::ready(server().with_results(6));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    let mut first = None;
    h.until(|h| {
        first = match h.core.next_batch(&stream, h.now, |v| v.rows()) {
            Ok(BatchPoll::Batch(rows)) => Some(rows),
            _ => None,
        };
        first.is_some()
    });
    assert_eq!(first, Some(2));
    h.core.cancel(stream);
    let other = begin(&mut h);
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
    let cancels = h
        .server
        .received()
        .iter()
        .filter(|f| {
            FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE)
                .is_ok_and(|v| v.command() == Command::Cancel)
        })
        .count();
    assert_eq!(cancels, 1);
    let next = h.core.read(&other, &read(&other), h.now).unwrap();
    assert_eq!(drain(&mut h, &next).len(), 6, "the other stream continues");
}

#[test]
fn test_a_stale_statement_is_refused_and_prepared_again() {
    let mut h = Harness::ready(server().with_results(1));
    let prepare = || {
        let mut terms = TermTable::new();
        let name = terms.iri("http://ex.org/name");
        Prepare::new(
            0,
            0,
            0,
            terms,
            QuadPattern::new(
                PatternSlot::DefaultGraph,
                PatternSlot::Parameter(0),
                PatternSlot::Bound(name),
                PatternSlot::Unbound,
            ),
            vec![ColumnType::Node],
            Vec::new(),
        )
    };
    let execute = |id| {
        let mut terms = TermTable::new();
        let a = terms.iri("http://ex.org/a");
        move |tx: &Transaction| Execute::new(id, tx.stream(), 0, terms.clone(), vec![a], Vec::new())
    };
    let p = h.core.prepare(&prepare(), h.now).unwrap();
    let Ok(Reply::Prepared(statement)) = h.settle(p) else {
        panic!()
    };
    let tx = begin(&mut h);
    let stream = h
        .core
        .execute(&tx, &execute(statement.id())(&tx), h.now)
        .unwrap();
    assert_eq!(drain(&mut h, &stream).len(), 1);
    h.server.set_schema(2);
    let reader = begin(&mut h);
    let other = h.core.read(&reader, &read(&reader), h.now).unwrap();
    let tx = begin(&mut h);
    let stale = h
        .core
        .execute(&tx, &execute(statement.id())(&tx), h.now)
        .unwrap();
    let mut refused = None;
    h.until(|h| {
        refused = h.core.next_batch(&stale, h.now, |_| ()).err();
        refused.is_some()
    });
    let refused = refused.unwrap();
    assert!(refused.is_stale() && !refused.retryable(), "{refused:?}");
    assert!(
        matches!(
            refused.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::StaleStatement,
                detail,
                ..
            } if detail == statement.id()
        ),
        "detail names the statement: {refused:?}"
    );
    assert!(h.core.is_ready(), "a refused request keeps the connection");
    assert_eq!(drain(&mut h, &other).len(), 1, "and the other stream");
    assert_eq!(
        h.core.next_batch(&stale, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stale.stream()
        },
        "the refusal ended its stream"
    );
    let goes_on = h.core.read(&tx, &read(&tx), h.now).unwrap();
    assert_eq!(
        drain(&mut h, &goes_on).len(),
        1,
        "and its transaction goes on"
    );
    let p = h.core.prepare(&prepare(), h.now).unwrap();
    let Ok(Reply::Prepared(again)) = h.settle(p) else {
        panic!()
    };
    assert_eq!((again.id(), again.schema()), (2, 2));
    let tx = begin(&mut h);
    let stream = h
        .core
        .execute(&tx, &execute(again.id())(&tx), h.now)
        .unwrap();
    assert_eq!(drain(&mut h, &stream).len(), 1);
}

/// Point 35: a statement belongs to the connection that prepared it, so the next connection holds none of its
/// predecessor's, and its ids start at 1 again.
#[test]
fn test_a_statement_does_not_outlive_its_connection() {
    let mut h = Harness::ready(server().with_results(1));
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    let prepare = Prepare::new(
        0,
        0,
        0,
        terms,
        QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Unbound,
            PatternSlot::Bound(name),
            PatternSlot::Unbound,
        ),
        Vec::new(),
        Vec::new(),
    );
    let p = h.core.prepare(&prepare, h.now).unwrap();
    let Ok(Reply::Prepared(statement)) = h.settle(p) else {
        panic!("a statement")
    };
    h.connect();
    h.until(|h| h.core.is_ready());
    let tx = begin(&mut h);
    let execute = Execute::new(
        statement.id(),
        tx.stream(),
        0,
        TermTable::new(),
        Vec::new(),
        Vec::new(),
    );
    let stream = h.core.execute(&tx, &execute, h.now).unwrap();
    let mut refused = None;
    h.until(|h| {
        refused = h.core.next_batch(&stream, h.now, |_| ()).err();
        refused.is_some()
    });
    assert!(
        matches!(
            refused.map(|e| e.0),
            Some(ClientErrorEnum::Refused {
                code: ErrorCode::UnknownStatement,
                detail: 1,
                ..
            })
        ),
        "the new connection holds no statement"
    );
    let p = h.core.prepare(&prepare, h.now).unwrap();
    let Ok(Reply::Prepared(again)) = h.settle(p) else {
        panic!("a statement")
    };
    assert_eq!(again.id(), 1, "ids start again on the new connection");
}

/// The server's Result frame on `stream`, numbered as the connection's next.
fn forged(h: &mut Harness, stream: u64, resume: u8, flags: u8) -> Vec<u8> {
    use ultragraphdb_protocol_wire::{BodyBuilder, ResultBatch};
    let frame = ResultBatch::new(1)
        .with_resume(vec![resume])
        .frame(
            Header::new(CLUSTER, Command::Result)
                .with_stream(stream)
                .with_flags(flags),
        )
        .unwrap();
    h.numbered(&frame)
}

/// Frames are numbered: a Result frame the network delivered twice is out of sequence, and the client gives the
/// connection up.
#[test]
fn test_a_repeated_frame_of_the_server_closes_the_connection() {
    let mut h = Harness::ready(server());
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    while h.core.transmit().is_some() {}
    let frame = forged(&mut h, stream.stream(), 1, 0);
    h.deliver(&frame).unwrap();
    let refused = h.deliver(&frame).unwrap_err();
    assert!(
        matches!(
            &refused.0,
            ClientErrorEnum::Protocol { error } if matches!(
                error.0,
                ProtocolErrorEnum::SequenceOrder { .. }
            )
        ),
        "{refused:?}"
    );
    assert!(!h.core.is_connected(), "the connection is given up");
}

/// A transaction's reads run on streams of their own: each names the transaction and has its own credit, and the
/// transaction commits on its own stream.
#[test]
fn test_two_reads_of_a_transaction_run_on_streams_of_their_own() {
    let mut h = Harness::ready(server().with_results(5));
    let (session, tx) = begin_writing(&mut h);
    let first = h.core.read(&tx, &read(&tx), h.now).unwrap();
    let second = h.core.read(&tx, &read(&tx), h.now).unwrap();
    assert!(first.stream() != tx.stream() && second.stream() != first.stream());
    assert_eq!(drain(&mut h, &first).len(), 5);
    assert_eq!(drain(&mut h, &second).len(), 5);
    let named: Vec<(u64, u64)> = h
        .server
        .received()
        .iter()
        .filter_map(|f| {
            let view = FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE).ok()?;
            let read = ReadView::from_frame(&view).ok()?;
            Some((view.header().stream(), read.transaction()))
        })
        .collect();
    assert_eq!(
        named,
        vec![
            (first.stream(), tx.stream()),
            (second.stream(), tx.stream())
        ]
    );
    assert_eq!(credits(&h), vec![2, 2], "each read granted its own credit");
    let p = h.core.commit(tx, session, h.now).unwrap();
    assert!(matches!(h.settle(p), Ok(Reply::Committed(_))));
}

/// Cancelling a read keeps its transaction.
#[test]
fn test_cancelling_a_read_keeps_its_transaction() {
    let mut h = Harness::ready(server().with_results(6));
    let (session, tx) = begin_writing(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    h.until(|h| {
        matches!(
            h.core.next_batch(&stream, h.now, |_| ()),
            Ok(BatchPoll::Batch(()))
        )
    });
    h.core.cancel(stream);
    let p = h.core.commit(tx, session, h.now).unwrap();
    assert!(matches!(h.settle(p), Ok(Reply::Committed(_))));
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
}

/// A transaction's end ends its reads: once its commit is answered, a read with frames left is finished.
#[test]
fn test_a_transactions_end_ends_its_reads() {
    let mut h = Harness::ready(server().with_results(8));
    let (session, tx) = begin_writing(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    let p = h.core.commit(tx, session, h.now).unwrap();
    assert!(matches!(h.settle(p), Ok(Reply::Committed(_))));
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
    let (_, aborted) = begin_writing(&mut h);
    let stream = h.core.read(&aborted, &read(&aborted), h.now).unwrap();
    h.core.abort(aborted).unwrap();
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        },
        "an abort ends them too"
    );
}

#[test]
fn test_a_read_naming_another_transaction_is_refused() {
    let mut h = Harness::ready(server());
    let tx = begin(&mut h);
    let other = begin(&mut h);
    assert_eq!(
        h.core.read(&tx, &read(&other), h.now).unwrap_err().0,
        ClientErrorEnum::TransactionMismatch {
            stream: tx.stream(),
            named: other.stream()
        }
    );
    let frames = h.server.received().len();
    h.step();
    assert_eq!(h.server.received().len(), frames, "nothing was sent");
}

/// The client numbers its frames from 1 on each connection, one more for each.
#[test]
fn test_the_clients_frames_are_numbered_from_one_on_each_connection() {
    let mut h = Harness::ready(server());
    let tx = begin(&mut h);
    let _ = h.core.read(&tx, &read(&tx), h.now).unwrap();
    h.step();
    let numbers: Vec<u128> = h
        .server
        .received()
        .iter()
        .map(|f| HeaderView::new(f).unwrap().sequence())
        .collect();
    assert_eq!(numbers, vec![1, 2, 3]);
    h.connect();
    h.until(|h| h.core.is_ready());
    let last = h.server.received().last().unwrap();
    assert_eq!(
        HeaderView::new(last).unwrap().sequence(),
        1,
        "a new connection's Hello"
    );
}

/// A frame of a stream that ended is late: the client drops it and keeps the connection.
#[test]
fn test_a_late_frame_of_a_cancelled_read_is_dropped() {
    let mut h = Harness::ready(server().with_results(1));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    while h.core.transmit().is_some() {}
    h.core.cancel(stream);
    let late = forged(&mut h, stream.stream(), 1, 0);
    h.deliver(&late).unwrap();
    assert!(h.core.is_ready(), "the connection stays");
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
}

/// A read starts with the Welcome's initial credit: no grant goes out before it is spent.
#[test]
fn test_a_read_grants_no_credit_before_its_initial_credit_is_spent() {
    let mut h = Harness::ready(server().with_results(3));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Wait)
    ));
    let queued: Vec<u16> = std::iter::from_fn(|| h.core.transmit())
        .map(|f| HeaderView::new(&f).unwrap().command())
        .collect();
    assert_eq!(
        queued,
        vec![Command::Read.number()],
        "the read alone, no Credit"
    );
}

/// A refusal on a transaction's stream ends the transaction, and its reads with it.
#[test]
fn test_a_refused_transaction_ends_its_reads() {
    use ultragraphdb_protocol_wire::{BodyBuilder, ErrorCode, ErrorReply, FLAG_LAST};
    let mut h = Harness::ready(server().with_results(1));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    while h.core.transmit().is_some() {}
    let refusal = ErrorReply::new(ErrorCode::Refused, "refused")
        .frame(
            Header::new(CLUSTER, Command::Error)
                .with_stream(tx.stream())
                .with_flags(FLAG_LAST),
        )
        .unwrap();
    let refusal = h.numbered(&refusal);
    h.deliver(&refusal).unwrap();
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
}

/// A transaction's end ends its own reads alone: another transaction's read goes on.
#[test]
fn test_a_transactions_end_leaves_another_transactions_read() {
    let mut h = Harness::ready(server().with_results(3));
    let (session, tx) = begin_writing(&mut h);
    let other = begin(&mut h);
    let mine = h.core.read(&tx, &read(&tx), h.now).unwrap();
    let theirs = h.core.read(&other, &read(&other), h.now).unwrap();
    let p = h.core.commit(tx, session, h.now).unwrap();
    assert!(matches!(h.settle(p), Ok(Reply::Committed(_))));
    assert_eq!(
        h.core.next_batch(&mine, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: mine.stream()
        }
    );
    assert_eq!(drain(&mut h, &theirs).len(), 3);
}

#[test]
fn test_a_read_matching_nothing_ends_after_one_empty_batch() {
    // Point 9: a read that matches nothing is answered with one batch flagged LAST and no rows.
    let mut h = Harness::ready(server().with_results(0));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    assert_eq!(drain(&mut h, &stream), vec![Vec::<String>::new()]);
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Done)
    ));
}

/// A grant adds the frames asked for to the read's credit, in a Credit frame of that many.
#[test]
fn test_a_grant_sends_a_credit_of_the_frames_asked_for() {
    let mut h = Harness::ready(server().with_results(3));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    h.core.grant(&stream, 7).unwrap();
    while h.step() {}
    assert_eq!(credits(&h), vec![7]);
    assert_eq!(drain(&mut h, &stream).len(), 3);
    assert_eq!(
        credits(&h),
        vec![7],
        "4 + 7 frames of credit cover the 3, so no grant follows"
    );
}

/// PROTOCOL.md section 9.2: a Credit grants at least 1 frame, so a grant of none is no request the client sends.
#[test]
fn test_a_grant_of_no_frames_is_refused_before_it_is_sent() {
    let mut h = Harness::ready(server().with_results(3));
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(&tx), h.now).unwrap();
    while h.step() {}
    assert_eq!(
        h.core.grant(&stream, 0).unwrap_err().0,
        ClientErrorEnum::Request {
            error: ultragraphdb_error_protocol::ProtocolError::FieldValue(14, "frames", 0)
        }
    );
    assert_eq!(h.core.transmit(), None, "no Credit queued");
    assert_eq!(drain(&mut h, &stream).len(), 3, "the read goes on");
}
