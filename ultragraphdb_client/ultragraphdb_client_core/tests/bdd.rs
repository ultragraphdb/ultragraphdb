/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire protocol's client-side scenarios: the client's core against the scripted server over the simulated
//! network, driven synchronously by the unit tests' harness, shared by path.

use cucumber::{World, then, when};
use ultragraphdb_client_core::{
    BatchPoll, ClientErrorEnum, Reply, ResultStream, ServerScript, Transaction,
};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::NetworkFaults;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, CommitView, Execute, FLAG_LAST, FrameView, Header,
    MAX_FRAME_SIZE, PatternSlot, Prepare, QuadPattern, Read, ResultBatch, ResultTerm, TermTable,
    Welcome,
};

mod utils {
    #[path = "harness.rs"]
    pub mod harness;
}

use crate::utils::harness::{CLUSTER, Harness, RELEASE, config, received};

#[derive(Default, World)]
struct Steps {
    harness: Option<Harness>,
    stream: Option<ResultStream>,
    other: Option<ResultStream>,
    outcome: Vec<u64>,
    refusal: Option<bool>,
    terms: usize,
}

impl std::fmt::Debug for Steps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Steps")
            .field("outcome", &self.outcome)
            .finish()
    }
}

fn begin(h: &mut Harness, read_only: bool) -> Transaction {
    let p = h.core.begin(0, read_only, h.now).unwrap();
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

/// The Result frames of `stream` the client has read, until none is ready.
fn batches(h: &mut Harness, stream: &ResultStream) -> usize {
    let mut read = 0;
    while let Ok(BatchPoll::Batch(())) = h.core.next_batch(stream, h.now, |_| ()) {
        read += 1;
    }
    read
}

/// Every remaining batch of \`stream\` until its last, each read as it arrives.
fn rest(h: &mut Harness, stream: &ResultStream) -> usize {
    let mut read = 0;
    h.until(|h| {
        loop {
            match h.core.next_batch(stream, h.now, |_| ()) {
                Ok(BatchPoll::Batch(())) => read += 1,
                Ok(BatchPoll::Done) => return true,
                _ => return false,
            }
        }
    });
    read
}

#[when("the connection drops after a commit is sent and the client reconnects and retries")]
fn lost_reply(w: &mut Steps) {
    let mut h = Harness::ready(ServerScript {
        drop_commits: 1,
        ..ServerScript::default()
    });
    let p = h.core.register(h.now).unwrap();
    let Ok(Reply::Registered(session)) = h.settle(p) else {
        panic!()
    };
    let tx = begin(&mut h, false);
    let p = h.core.commit(tx, session, h.now).unwrap();
    let Ok(Reply::Committed(c)) = h.settle(p) else {
        panic!("the reply")
    };
    w.outcome = vec![c.request(), h.server.applied(), u64::from(h.connections)];
    let retried: Vec<(u64, u64)> = h
        .server
        .received()
        .iter()
        .filter_map(|f| {
            FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE)
                .ok()
                .and_then(|v| {
                    CommitView::from_frame(&v)
                        .ok()
                        .map(|c| (c.session(), c.request()))
                })
        })
        .collect();
    assert_eq!(
        retried,
        vec![(9, 1), (9, 1)],
        "retried with the same session and request"
    );
}

#[then("the commit is applied once and the client receives its reply")]
fn applied_once(w: &mut Steps) {
    assert_eq!(w.outcome, vec![1, 1, 2]);
}

#[when("a client grants credit for two frames of a result of ten")]
fn credit_two(w: &mut Steps) {
    let mut h = Harness::with(
        config().with_credit(2),
        Welcome::new(1, RELEASE, "s")
            .with_max_frame(1 << 20)
            .with_max_streams(8)
            .with_initial_credit(0),
        ServerScript {
            results: 10,
            ..ServerScript::default()
        },
        NetworkFaults::none(),
        1,
    );
    h.connect();
    h.until(|h| h.core.is_ready());
    let tx = begin(&mut h, true);
    let stream = h.core.read(&tx, &read(), h.now).unwrap();
    assert!(
        matches!(
            h.core.next_batch(&stream, h.now, |_| ()),
            Ok(BatchPoll::Wait)
        ),
        "a grant is sent"
    );
    while h.step() {}
    w.outcome = vec![batches(&mut h, &stream) as u64];
    while h.step() {}
    w.outcome.push(batches(&mut h, &stream) as u64);
    w.harness = Some(h);
    w.stream = Some(stream);
}

#[then("the server sends two frames and waits until more credit arrives")]
fn two_then_wait(w: &mut Steps) {
    assert_eq!(
        w.outcome,
        vec![2, 2],
        "two frames per grant: the second grant went with the read that found none"
    );
    let (Some(h), Some(stream)) = (w.harness.as_mut(), w.stream.as_ref()) else {
        panic!()
    };
    assert_eq!(rest(h, stream), 6);
}

#[when("a client cancels a stream mid-result")]
fn cancel(w: &mut Steps) {
    let mut h = Harness::ready(ServerScript {
        results: 6,
        ..ServerScript::default()
    });
    let (a, b) = (begin(&mut h, true), begin(&mut h, true));
    let (sa, sb) = (
        h.core.read(&a, &read(), h.now).unwrap(),
        h.core.read(&b, &read(), h.now).unwrap(),
    );
    let mut first = false;
    h.until(|h| {
        first = matches!(
            h.core.next_batch(&sa, h.now, |_| ()),
            Ok(BatchPoll::Batch(()))
        );
        first
    });
    h.core.cancel(sa);
    w.harness = Some(h);
    w.stream = Some(sa);
    w.other = Some(sb);
}

#[then("no further frame of that stream arrives, and other streams continue")]
fn cancelled(w: &mut Steps) {
    let (Some(h), Some(sa), Some(sb)) = (w.harness.as_mut(), w.stream, w.other) else {
        panic!()
    };
    assert_eq!(rest(h, &sb), 6);
    assert_eq!(
        h.core.next_batch(&sa, h.now, |_| ()).unwrap_err().0,
        ClientErrorEnum::StreamFinished {
            stream: sa.stream()
        }
    );
}

#[when(
    "a statement is prepared, the catalog of its namespace changes, and the statement is executed"
)]
fn stale(w: &mut Steps) {
    let mut h = Harness::ready(ServerScript {
        results: 1,
        ..ServerScript::default()
    });
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    let prepare = Prepare::new(
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
    );
    let p = h.core.prepare(&prepare, h.now).unwrap();
    let Ok(Reply::Prepared(statement)) = h.settle(p) else {
        panic!()
    };
    h.server.set_schema(2);
    let tx = begin(&mut h, true);
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let stream = h
        .core
        .execute(
            &tx,
            &Execute::new(statement.id(), 0, 0, terms, vec![a], Vec::new()),
            h.now,
        )
        .unwrap();
    let mut refused = None;
    h.until(|h| {
        refused = h.core.next_batch(&stream, h.now, |_| ()).err();
        refused.is_some()
    });
    w.refusal = refused.map(|e| e.is_stale());
    let p = h.core.prepare(&prepare, h.now).unwrap();
    let again = h.settle(p);
    w.outcome = match again {
        Ok(Reply::Prepared(s)) => vec![s.schema()],
        _ => Vec::new(),
    };
}

#[then("the execution is refused as stale, and preparing it again succeeds")]
fn refused_stale(w: &mut Steps) {
    assert_eq!(w.refusal, Some(true));
    assert_eq!(w.outcome, vec![2]);
}

#[when("a batch of 1,000 rows names the same IRI in every row")]
fn repeated(w: &mut Steps) {
    let mut h = Harness::ready(ServerScript::default());
    let tx = begin(&mut h, true);
    let stream = h.core.read(&tx, &read(), h.now).unwrap();
    while h.core.transmit().is_some() {}
    let iri = ResultTerm::new(EncodedTerm::uid(42)).with_text("http://ex.org/same");
    let mut batch = ResultBatch::new(1);
    for _ in 0..1_000 {
        batch.push_row(&[Some(&iri)]).unwrap();
    }
    let frame = batch
        .frame(
            Header::new(CLUSTER, Command::Result)
                .with_stream(stream.stream())
                .with_flags(FLAG_LAST),
        )
        .unwrap();
    h.core
        .receive(received(&h.pool, &frame).unwrap(), h.now)
        .unwrap();
    let Ok(BatchPoll::Batch((rows, terms, same))) = h.core.next_batch(&stream, h.now, |v| {
        let same = (0..v.rows()).all(|r| {
            v.cell(0, r).and_then(|c| v.term(c)).and_then(|t| t.text())
                == Some("http://ex.org/same")
        });
        (v.rows(), v.term_count(), same)
    }) else {
        panic!("the batch")
    };
    assert!(same && rows == 1_000);
    w.terms = terms;
}

#[then("its term table holds that IRI once")]
fn once(w: &mut Steps) {
    assert_eq!(w.terms, 1);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
