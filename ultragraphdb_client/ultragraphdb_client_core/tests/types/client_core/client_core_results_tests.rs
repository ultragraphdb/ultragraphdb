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

use crate::utils::harness::{
    CLUSTER, Harness, RELEASE, begun, config, prepared, registered, server,
};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

fn begin(h: &mut Harness) -> TestResult<Transaction> {
    let p = h.core.begin(0, true, h.now)?;
    begun(h.settle(p)?)
}

fn begin_writing(h: &mut Harness) -> TestResult<(Session, Transaction)> {
    let registering = h.core.register(h.now)?;
    let session = registered(h.settle(registering)?)?;
    let beginning = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(beginning)?)?;
    Ok((session, tx))
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
fn drain(h: &mut Harness, stream: &ResultStream) -> TestResult<Vec<Vec<String>>> {
    let mut batches = Vec::new();
    let mut done = false;
    let mut failure = None;
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
                Err(error) => {
                    failure = Some(error);
                    return true;
                }
            }
        }
    })?;
    if let Some(error) = failure {
        return Err(TestError::new(format!(
            "a batch, the end or a wait: {error}"
        )));
    }
    assert!(done);
    Ok(batches)
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
fn test_a_read_streams_its_batches_read_in_place_until_the_last() -> TestResult {
    let mut h = Harness::ready(server().with_results(5))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let batches = drain(&mut h, &stream)?;
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
    Ok(())
}

/// Credit stops a stream, from the client's side: with no initial credit the client grants its window first, and the
/// server sends that many frames before it waits.
#[test]
fn test_without_credit_the_client_grants_before_any_frame_arrives() -> TestResult {
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
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Wait)
    ));
    assert_eq!(drain(&mut h, &stream)?.len(), 3);
    assert_eq!(credits(&h), vec![2, 2]);
    assert_eq!(
        h.core.grant(&stream, 1).err().context("an error")?.0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        },
        "a finished read takes no credit"
    );
    Ok(())
}

#[test]
fn test_cancel_ends_the_stream_and_its_later_frames_are_dropped() -> TestResult {
    let mut h = Harness::ready(server().with_results(6))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let mut first = None;
    h.until(|h| {
        first = match h.core.next_batch(&stream, h.now, |v| v.rows()) {
            Ok(BatchPoll::Batch(rows)) => Some(rows),
            _ => None,
        };
        first.is_some()
    })?;
    assert_eq!(first, Some(2));
    h.core.cancel(stream);
    let other = begin(&mut h)?;
    assert_eq!(
        h.core
            .next_batch(&stream, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
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
    let next = h.core.read(&other, &read(&other), h.now)?;
    assert_eq!(drain(&mut h, &next)?.len(), 6, "the other stream continues");
    Ok(())
}

/// A Prepare of the pattern `?s <http://ex.org/name> ?o` in the default graph, its subject a node parameter.
fn prepare_name() -> Prepare {
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
}

/// An Execute of statement `id` in `tx`, its parameter `http://ex.org/a`.
fn execute(id: u64, tx: &Transaction) -> Execute {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Execute::new(id, tx.stream(), 0, terms, vec![a], Vec::new())
}

#[test]
fn test_a_stale_statement_is_refused_and_prepared_again() -> TestResult {
    let mut h = Harness::ready(server().with_results(1))?;
    let p = h.core.prepare(&prepare_name(), h.now)?;
    let statement = prepared(h.settle(p)?)?;
    let tx = begin(&mut h)?;
    let stream = h.core.execute(&tx, &execute(statement.id(), &tx), h.now)?;
    assert_eq!(drain(&mut h, &stream)?.len(), 1);
    h.server.set_schema(2);
    let reader = begin(&mut h)?;
    let other = h.core.read(&reader, &read(&reader), h.now)?;
    let stale_tx = begin(&mut h)?;
    let stale = h
        .core
        .execute(&stale_tx, &execute(statement.id(), &stale_tx), h.now)?;
    let mut refused = None;
    h.until(|h| {
        refused = h.core.next_batch(&stale, h.now, |_| ()).err();
        refused.is_some()
    })?;
    let refused = refused.context("a refusal")?;
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
    assert_eq!(drain(&mut h, &other)?.len(), 1, "and the other stream");
    assert_eq!(
        h.core
            .next_batch(&stale, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: stale.stream()
        },
        "the refusal ended its stream"
    );
    let goes_on = h.core.read(&stale_tx, &read(&stale_tx), h.now)?;
    assert_eq!(
        drain(&mut h, &goes_on)?.len(),
        1,
        "and its transaction goes on"
    );
    let preparing = h.core.prepare(&prepare_name(), h.now)?;
    let again = prepared(h.settle(preparing)?)?;
    assert_eq!((again.id(), again.schema()), (2, 2));
    let current_tx = begin(&mut h)?;
    let current = h
        .core
        .execute(&current_tx, &execute(again.id(), &current_tx), h.now)?;
    assert_eq!(drain(&mut h, &current)?.len(), 1);
    Ok(())
}

/// Point 35: a statement belongs to the connection that prepared it, so the next connection holds none of its
/// predecessor's, and its ids start at 1 again.
#[test]
fn test_a_statement_does_not_outlive_its_connection() -> TestResult {
    let mut h = Harness::ready(server().with_results(1))?;
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
    let p = h.core.prepare(&prepare, h.now)?;
    let statement = prepared(h.settle(p)?)?;
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let tx = begin(&mut h)?;
    let execute = Execute::new(
        statement.id(),
        tx.stream(),
        0,
        TermTable::new(),
        Vec::new(),
        Vec::new(),
    );
    let stream = h.core.execute(&tx, &execute, h.now)?;
    let mut refused = None;
    h.until(|h| {
        refused = h.core.next_batch(&stream, h.now, |_| ()).err();
        refused.is_some()
    })?;
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
    let preparing = h.core.prepare(&prepare, h.now)?;
    let again = prepared(h.settle(preparing)?)?;
    assert_eq!(again.id(), 1, "ids start again on the new connection");
    Ok(())
}

/// The server's Result frame on `stream`, numbered as the connection's next.
fn forged(h: &mut Harness, stream: u64, resume: u8, flags: u8) -> TestResult<Vec<u8>> {
    use ultragraphdb_protocol_wire::{BodyBuilder, ResultBatch};
    let frame = ResultBatch::new(1).with_resume(vec![resume]).frame(
        Header::new(CLUSTER, Command::Result)
            .with_stream(stream)
            .with_flags(flags),
    )?;
    h.numbered(&frame)
}

/// Frames are numbered: a Result frame the network delivered twice is out of sequence, and the client gives the
/// connection up.
#[test]
fn test_a_repeated_frame_of_the_server_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.core.transmit().is_some() {}
    let frame = forged(&mut h, stream.stream(), 1, 0)?;
    h.deliver(&frame)??;
    let refused = h.deliver(&frame)?.err().context("an error")?;
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
    Ok(())
}

/// A transaction's reads run on streams of their own: each names the transaction and has its own credit, and the
/// transaction commits on its own stream.
#[test]
fn test_two_reads_of_a_transaction_run_on_streams_of_their_own() -> TestResult {
    let mut h = Harness::ready(server().with_results(5))?;
    let (session, tx) = begin_writing(&mut h)?;
    let first = h.core.read(&tx, &read(&tx), h.now)?;
    let second = h.core.read(&tx, &read(&tx), h.now)?;
    assert!(first.stream() != tx.stream() && second.stream() != first.stream());
    assert_eq!(drain(&mut h, &first)?.len(), 5);
    assert_eq!(drain(&mut h, &second)?.len(), 5);
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
    let p = h.core.commit(tx, session, h.now)?;
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    Ok(())
}

/// Cancelling a read keeps its transaction.
#[test]
fn test_cancelling_a_read_keeps_its_transaction() -> TestResult {
    let mut h = Harness::ready(server().with_results(6))?;
    let (session, tx) = begin_writing(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    h.until(|h| {
        matches!(
            h.core.next_batch(&stream, h.now, |_| ()),
            Ok(BatchPoll::Batch(()))
        )
    })?;
    h.core.cancel(stream);
    let p = h.core.commit(tx, session, h.now)?;
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    assert_eq!(
        h.core
            .next_batch(&stream, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
    Ok(())
}

/// A transaction's end ends its reads: once its commit is answered, a read with frames left is finished.
#[test]
fn test_a_transactions_end_ends_its_reads() -> TestResult {
    let mut h = Harness::ready(server().with_results(8))?;
    let (session, tx) = begin_writing(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let p = h.core.commit(tx, session, h.now)?;
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    assert_eq!(
        h.core
            .next_batch(&stream, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
    let (_, aborted) = begin_writing(&mut h)?;
    let aborted_stream = h.core.read(&aborted, &read(&aborted), h.now)?;
    h.core.abort(aborted)?;
    assert_eq!(
        h.core
            .next_batch(&aborted_stream, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: aborted_stream.stream()
        },
        "an abort ends them too"
    );
    Ok(())
}

#[test]
fn test_a_read_naming_another_transaction_is_refused() -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h)?;
    let other = begin(&mut h)?;
    assert_eq!(
        h.core
            .read(&tx, &read(&other), h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::TransactionMismatch {
            stream: tx.stream(),
            named: other.stream()
        }
    );
    let frames = h.server.received().len();
    let _moved: bool = h.step()?;
    assert_eq!(h.server.received().len(), frames, "nothing was sent");
    Ok(())
}

/// The client numbers its frames from 1 on each connection, one more for each.
#[test]
fn test_the_clients_frames_are_numbered_from_one_on_each_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h)?;
    let _stream: ResultStream = h.core.read(&tx, &read(&tx), h.now)?;
    let _moved: bool = h.step()?;
    let numbers: Vec<u128> = h
        .server
        .received()
        .iter()
        .map(|f| Ok(HeaderView::new(f)?.sequence()))
        .collect::<TestResult<_>>()?;
    assert_eq!(numbers, vec![1, 2, 3]);
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let last = h.server.received().last().context("a frame received")?;
    assert_eq!(
        HeaderView::new(last)?.sequence(),
        1,
        "a new connection's Hello"
    );
    Ok(())
}

/// A frame of a stream that ended is late: the client drops it and keeps the connection.
#[test]
fn test_a_late_frame_of_a_cancelled_read_is_dropped() -> TestResult {
    let mut h = Harness::ready(server().with_results(1))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.core.transmit().is_some() {}
    h.core.cancel(stream);
    let late = forged(&mut h, stream.stream(), 1, 0)?;
    h.deliver(&late)??;
    assert!(h.core.is_ready(), "the connection stays");
    assert_eq!(
        h.core
            .next_batch(&stream, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
    Ok(())
}

/// A read starts with the Welcome's initial credit: no grant goes out before it is spent.
#[test]
fn test_a_read_grants_no_credit_before_its_initial_credit_is_spent() -> TestResult {
    let mut h = Harness::ready(server().with_results(3))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Wait)
    ));
    let queued: Vec<u16> = std::iter::from_fn(|| h.core.transmit())
        .map(|f| Ok(HeaderView::new(&f)?.command()))
        .collect::<TestResult<_>>()?;

    assert_eq!(
        queued,
        vec![Command::Read.number()],
        "the read alone, no Credit"
    );
    Ok(())
}

/// A refusal on a transaction's stream ends the transaction, and its reads with it.
#[test]
fn test_a_refused_transaction_ends_its_reads() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyBuilder, ErrorCode, ErrorReply, FLAG_LAST};
    let mut h = Harness::ready(server().with_results(1))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.core.transmit().is_some() {}
    let refusal = ErrorReply::new(ErrorCode::Refused, "refused").frame(
        Header::new(CLUSTER, Command::Error)
            .with_stream(tx.stream())
            .with_flags(FLAG_LAST),
    )?;
    let refusal = h.numbered(&refusal)?;
    h.deliver(&refusal)??;
    assert_eq!(
        h.core
            .next_batch(&stream, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: stream.stream()
        }
    );
    Ok(())
}

/// A transaction's end ends its own reads alone: another transaction's read goes on.
#[test]
fn test_a_transactions_end_leaves_another_transactions_read() -> TestResult {
    let mut h = Harness::ready(server().with_results(3))?;
    let (session, tx) = begin_writing(&mut h)?;
    let other = begin(&mut h)?;
    let mine = h.core.read(&tx, &read(&tx), h.now)?;
    let theirs = h.core.read(&other, &read(&other), h.now)?;
    let p = h.core.commit(tx, session, h.now)?;
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    assert_eq!(
        h.core
            .next_batch(&mine, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: mine.stream()
        }
    );
    assert_eq!(drain(&mut h, &theirs)?.len(), 3);
    Ok(())
}

#[test]
fn test_a_read_matching_nothing_ends_after_one_empty_batch() -> TestResult {
    // Point 9: a read that matches nothing is answered with one batch flagged LAST and no rows.
    let mut h = Harness::ready(server().with_results(0))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    assert_eq!(drain(&mut h, &stream)?, vec![Vec::<String>::new()]);
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Done)
    ));
    Ok(())
}

/// A grant adds the frames asked for to the read's credit, in a Credit frame of that many.
#[test]
fn test_a_grant_sends_a_credit_of_the_frames_asked_for() -> TestResult {
    let mut h = Harness::ready(server().with_results(3))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    h.core.grant(&stream, 7)?;
    while h.step()? {}
    assert_eq!(credits(&h), vec![7]);
    assert_eq!(drain(&mut h, &stream)?.len(), 3);
    assert_eq!(
        credits(&h),
        vec![7],
        "4 + 7 frames of credit cover the 3, so no grant follows"
    );
    Ok(())
}

/// PROTOCOL.md section 9.2: a Credit grants at least 1 frame, so a grant of none is no request the client sends.
#[test]
fn test_a_grant_of_no_frames_is_refused_before_it_is_sent() -> TestResult {
    let mut h = Harness::ready(server().with_results(3))?;
    let tx = begin(&mut h)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.step()? {}
    assert_eq!(
        h.core.grant(&stream, 0).err().context("an error")?.0,
        ClientErrorEnum::Request {
            error: ultragraphdb_error_protocol::ProtocolError::field_value(14, "frames", 0)
        }
    );
    assert_eq!(h.core.transmit(), None, "no Credit queued");
    assert_eq!(drain(&mut h, &stream)?.len(), 3, "the read goes on");
    Ok(())
}

/// A frame on a read's stream that is no Result, an Error whose body breaks its layout, or a Result whose chunk does
/// not verify gives the connection up with the rule it broke, and the read with it.
#[test]
fn test_a_broken_frame_on_a_read_gives_the_connection_up() -> TestResult {
    use ultragraphdb_protocol_wire::{Begin, BodyBuilder, FLAG_LAST};
    for case in 0..3 {
        let mut h = Harness::ready(server())?;
        let tx = begin(&mut h)?;
        let stream = h.core.read(&tx, &read(&tx), h.now)?;
        while h.core.transmit().is_some() {}
        let header = Header::new(CLUSTER, Command::Result)
            .with_stream(stream.stream())
            .with_flags(FLAG_LAST);
        let frame = match case {
            0 => Begin::new(0, 5, true).frame(header.with_command(Command::Begin))?,
            1 => header
                .with_command(Command::Error)
                .encode_frame(&[0xFF; 3])?,
            _ => {
                let mut frame = forged(&mut h, stream.stream(), 1, FLAG_LAST)?;
                let last = frame.len() - 1;
                frame[last] ^= 4;
                frame
            }
        };
        let frame = if case == 2 {
            frame
        } else {
            h.numbered(&frame)?
        };
        h.deliver(&frame)??;
        let refused = h
            .core
            .next_batch(&stream, h.now, |_| ())
            .err()
            .context("an error")?;
        let rule = match case {
            2 => {
                matches!(&refused.0, ClientErrorEnum::Protocol { error } if matches!(error.0, ProtocolErrorEnum::Chunk { index: 0, .. }))
            }
            _ => {
                matches!(&refused.0, ClientErrorEnum::Protocol { error } if !matches!(error.0, ProtocolErrorEnum::Chunk { .. }))
            }
        };
        assert!(rule, "case {case}: {refused:?}");
        assert!(!h.core.is_connected(), "case {case}: given up");
        assert_eq!(
            h.core
                .next_batch(&stream, h.now, |_| ())
                .err()
                .context("an error")?
                .0,
            ClientErrorEnum::StreamFinished {
                stream: stream.stream()
            },
            "case {case}"
        );
    }
    Ok(())
}

/// The core of the tests, welcomed by a server allowing one stream and frames up to 524,288 bytes, the least it may.
fn narrow() -> TestResult<Harness> {
    let mut h = Harness::with(
        config(),
        TestServer::new(
            CLUSTER,
            Welcome::new(1, RELEASE, "s")
                .with_max_frame(524_288)
                .with_max_streams(1)
                .with_initial_credit(4),
        ),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    Ok(h)
}

/// Whether `error` refuses a request past the Welcome's 524,288 bytes.
fn too_large(error: &ultragraphdb_client_core::ClientError) -> bool {
    matches!(&error.0, ClientErrorEnum::Request { error } if matches!(error.0, ProtocolErrorEnum::FrameTooLarge { limit: 524_288, .. }))
}

/// A request on a stream of its own past the Welcome's largest frame is refused before it is sent, and the stream it
/// took is free again: Prepare, Drop and a read alike.
#[test]
fn test_a_request_too_large_frees_its_stream() -> TestResult {
    let mut h = narrow()?;
    let long = format!("http://ex.org/{}", "x".repeat(530_000));
    let mut terms = TermTable::new();
    let name = terms.iri(&long);
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Unbound,
        PatternSlot::Bound(name),
        PatternSlot::Unbound,
    );
    let prepare = Prepare::new(0, 0, 0, terms.clone(), pattern, Vec::new(), Vec::new());
    let refused = h.core.prepare(&prepare, h.now).err().context("an error")?;
    assert!(too_large(&refused), "{refused:?}");
    assert!(h.core.transmit().is_none(), "nothing sent");
    let registering = h.core.register(h.now)?;
    let session = registered(h.settle(registering)?)?;
    let dropping = ultragraphdb_protocol_wire::DropCommand::predicate(0, long.clone());
    let drop_refused = h
        .core
        .drop_command(dropping, session, h.now)
        .err()
        .context("an error")?;
    assert!(too_large(&drop_refused), "{drop_refused:?}");
    assert_eq!(h.core.next_request(session), Some(1), "no drop in flight");
    let tx = begin(&mut h)?;
    // The transaction holds the one stream: a read needs another, so it is refused at the limit before its frame.
    assert_eq!(
        h.core
            .read(&tx, &read(&tx), h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamLimit { limit: 1 }
    );
    Ok(())
}

/// A read past the Welcome's largest frame is refused before it is sent, and its stream is free again.
#[test]
fn test_a_read_too_large_frees_its_stream() -> TestResult {
    let mut h = Harness::with(
        config(),
        TestServer::new(
            CLUSTER,
            Welcome::new(1, RELEASE, "s")
                .with_max_frame(524_288)
                .with_max_streams(2)
                .with_initial_credit(4),
        ),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let tx = begin(&mut h)?;
    let mut terms = TermTable::new();
    let a = terms.iri(&format!("http://ex.org/{}", "x".repeat(530_000)));
    let large = Read::new(
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
    );
    let refused = h.core.read(&tx, &large, h.now).err().context("an error")?;
    assert!(too_large(&refused), "{refused:?}");
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    assert_eq!(
        stream.stream(),
        tx.stream() + 2,
        "the refused read's stream number is not reused"
    );
    Ok(())
}
