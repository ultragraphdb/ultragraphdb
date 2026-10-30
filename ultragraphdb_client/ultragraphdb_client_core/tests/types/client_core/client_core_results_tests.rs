/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{
    BatchPoll, ClientErrorEnum, Reply, ResultStream, ServerScript, Transaction,
};
use ultragraphdb_protocol_transport::NetworkFaults;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyView, ColumnType, Command, CreditView, Execute, FrameView, MAX_FRAME_SIZE,
    PatternSlot, Prepare, QuadPattern, Read, TermTable, Welcome,
};

use crate::utils::harness::{CLUSTER, Harness, RELEASE, config};

fn begin(h: &mut Harness) -> Transaction {
    let p = h.core.begin(0, true, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!("a transaction")
    };
    tx
}

fn read() -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Read::new(
        0,
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
    let mut h = Harness::ready(ServerScript {
        results: 5,
        ..ServerScript::default()
    });
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(), h.now).unwrap();
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
    let mut h = Harness::with(
        config(),
        Welcome::new(1, RELEASE, "s")
            .with_max_frame(1 << 20)
            .with_max_streams(8)
            .with_initial_credit(0),
        ServerScript {
            results: 3,
            ..ServerScript::default()
        },
        NetworkFaults::none(),
        1,
    );
    h.connect();
    h.until(|h| h.core.is_ready());
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(), h.now).unwrap();
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Wait)
    ));
    assert_eq!(drain(&mut h, &stream).len(), 3);
    assert_eq!(credits(&h), vec![2, 2]);
    h.core.grant(&stream, 1).unwrap_err();
}

#[test]
fn test_cancel_ends_the_stream_and_its_later_frames_are_dropped() {
    let mut h = Harness::ready(ServerScript {
        results: 6,
        ..ServerScript::default()
    });
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(), h.now).unwrap();
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
    let next = h.core.read(&other, &read(), h.now).unwrap();
    assert_eq!(drain(&mut h, &next).len(), 6, "the other stream continues");
}

#[test]
fn test_a_stale_statement_is_refused_and_prepared_again() {
    let mut h = Harness::ready(ServerScript {
        results: 1,
        ..ServerScript::default()
    });
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
        Execute::new(id, 0, 0, terms, vec![a], Vec::new())
    };
    let p = h.core.prepare(&prepare(), h.now).unwrap();
    let Ok(Reply::Prepared(statement)) = h.settle(p) else {
        panic!()
    };
    let tx = begin(&mut h);
    let stream = h
        .core
        .execute(&tx, &execute(statement.id()), h.now)
        .unwrap();
    assert_eq!(drain(&mut h, &stream).len(), 1);
    h.server.set_schema(2);
    let reader = begin(&mut h);
    let other = h.core.read(&reader, &read(), h.now).unwrap();
    let tx = begin(&mut h);
    let stale = h
        .core
        .execute(&tx, &execute(statement.id()), h.now)
        .unwrap();
    let mut refused = None;
    h.until(|h| {
        refused = h.core.next_batch(&stale, h.now, |_| ()).err();
        refused.is_some()
    });
    let refused = refused.unwrap();
    assert!(refused.is_stale() && !refused.retryable(), "{refused:?}");
    assert!(h.core.is_ready(), "a refused request keeps the connection");
    assert_eq!(drain(&mut h, &other).len(), 1, "and the other stream");
    assert!(
        h.core.read(&tx, &read(), h.now).is_err(),
        "the refusal ended its stream"
    );
    let p = h.core.prepare(&prepare(), h.now).unwrap();
    let Ok(Reply::Prepared(again)) = h.settle(p) else {
        panic!()
    };
    assert_eq!((again.id(), again.schema()), (2, 2));
    let tx = begin(&mut h);
    let stream = h.core.execute(&tx, &execute(again.id()), h.now).unwrap();
    assert_eq!(drain(&mut h, &stream).len(), 1);
}

/// A Result frame the network delivered twice in a row is read once.
#[test]
fn test_a_duplicated_result_frame_is_read_once() {
    use ultragraphdb_protocol_wire::{BodyBuilder, FLAG_LAST, Header, ResultBatch};
    let mut h = Harness::ready(ServerScript::default());
    let tx = begin(&mut h);
    let stream = h.core.read(&tx, &read(), h.now).unwrap();
    while h.core.transmit().is_some() {}
    let batch = |resume: u8, flags| {
        ResultBatch::new(1)
            .with_resume(vec![resume])
            .frame(
                Header::new(CLUSTER, Command::Result)
                    .with_stream(stream.stream())
                    .with_flags(flags),
            )
            .unwrap()
    };
    let (first, last) = (batch(1, 0), batch(2, FLAG_LAST));
    for frame in [&first, &first, &last] {
        h.core
            .receive(
                crate::utils::harness::received(&h.pool, frame).unwrap(),
                h.now,
            )
            .unwrap();
    }
    let mut resumes = Vec::new();
    while let Ok(BatchPoll::Batch(resume)) =
        h.core.next_batch(&stream, h.now, |v| v.resume().to_vec())
    {
        resumes.push(resume);
    }
    assert_eq!(resumes, vec![vec![1], vec![2]]);
}
