/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire protocol's client-side scenarios: the client's core against the scripted server over the simulated
//! network, driven synchronously by the unit tests' harness, shared by path.

use cucumber::{World, then, when};
use std::time::Duration;

use ultragraphdb_client_core::{
    BatchPoll, ClientErrorEnum, Reply, ResultStream, Session, TEST_PRINCIPAL, TestServer,
    Transaction,
};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::{Anonymous, Login, NetworkFaults};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, CommitView, ErrorCode, Execute, FLAG_LAST,
    FrameView, Header, MAX_FRAME_SIZE, PatternSlot, Prepare, QuadPattern, Read, ResultBatch,
    ResultTerm, TermTable, Welcome,
};

mod utils {
    // The unit tests' harness, shared by path: each target uses a part of it.
    #[allow(dead_code)]
    #[path = "harness.rs"]
    pub mod harness;
}

use crate::utils::harness::{
    CLUSTER, Harness, RELEASE, begun, committed, config, prepared, registered, scram_welcome,
    server,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

#[derive(Default, World)]
struct Steps {
    harness: Option<Harness>,
    transaction: Option<Transaction>,
    session: Option<Session>,
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

fn begin(h: &mut Harness, read_only: bool) -> TestResult<Transaction> {
    let p = h.core.begin(0, read_only, h.now)?;
    begun(h.settle(p)?)
}

fn session(h: &mut Harness) -> TestResult<Session> {
    let p = h.core.register(h.now)?;
    registered(h.settle(p)?)
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

/// The Result frames of `stream` the client has read, until none is ready.
fn batches(h: &mut Harness, stream: &ResultStream) -> usize {
    let mut read = 0;
    while let Ok(BatchPoll::Batch(())) = h.core.next_batch(stream, h.now, |_| ()) {
        read += 1;
    }
    read
}

/// Every remaining batch of \`stream\` until its last, each read as it arrives.
fn rest<L: Login>(h: &mut Harness<L>, stream: &ResultStream) -> TestResult<usize> {
    let mut read = 0;
    h.until(|h| {
        loop {
            match h.core.next_batch(stream, h.now, |_| ()) {
                Ok(BatchPoll::Batch(())) => read += 1,
                Ok(BatchPoll::Done) => return true,
                _ => return false,
            }
        }
    })?;
    Ok(read)
}

#[when("the connection drops after a commit is sent and the client reconnects and retries")]
fn lost_reply(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, session, h.now)?;
    let c = committed(h.settle(p)?)?;
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
                        .map(|commit| (commit.session(), commit.request()))
                })
        })
        .collect();
    assert_eq!(
        retried,
        vec![(9, 1), (9, 1)],
        "retried with the same session and request"
    );
    Ok(())
}

#[then("the commit is applied once and the client receives its reply")]
fn applied_once(w: &mut Steps) -> TestResult {
    assert_eq!(w.outcome, vec![1, 1, 2]);
    Ok(())
}

#[when("a client grants credit for two frames of a result of ten")]
fn credit_two(w: &mut Steps) -> TestResult {
    let welcome = Welcome::new(1, RELEASE, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(8)
        .with_initial_credit(0);
    let mut h = Harness::with(
        config().with_credit(2),
        TestServer::new(CLUSTER, welcome).with_results(10),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let tx = begin(&mut h, true)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    assert!(
        matches!(
            h.core.next_batch(&stream, h.now, |_| ()),
            Ok(BatchPoll::Wait)
        ),
        "a grant is sent"
    );
    while h.step()? {}
    w.outcome = vec![batches(&mut h, &stream) as u64];
    while h.step()? {}
    w.outcome.push(batches(&mut h, &stream) as u64);
    w.harness = Some(h);
    w.stream = Some(stream);
    Ok(())
}

#[then("the server sends two frames and waits until more credit arrives")]
fn two_then_wait(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.outcome,
        vec![2, 2],
        "two frames per grant: the second grant went with the read that found none"
    );
    let h = w.harness.as_mut().context("a harness")?;
    let stream = w.stream.as_ref().context("a stream")?;
    assert_eq!(rest(h, stream)?, 6);
    Ok(())
}

#[when("a client cancels a stream mid-result")]
fn cancel(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server().with_results(6))?;
    let (a, b) = (begin(&mut h, true)?, begin(&mut h, true)?);
    let (sa, sb) = (
        h.core.read(&a, &read(&a), h.now)?,
        h.core.read(&b, &read(&b), h.now)?,
    );
    let mut first = false;
    h.until(|h| {
        first = matches!(
            h.core.next_batch(&sa, h.now, |_| ()),
            Ok(BatchPoll::Batch(()))
        );
        first
    })?;
    h.core.cancel(sa);
    w.harness = Some(h);
    w.stream = Some(sa);
    w.other = Some(sb);
    Ok(())
}

#[then("no further frame of that stream arrives, and other streams continue")]
fn cancelled(w: &mut Steps) -> TestResult {
    let h = w.harness.as_mut().context("a harness")?;
    let (sa, sb) = (
        w.stream.context("a stream")?,
        w.other.context("another stream")?,
    );
    assert_eq!(rest(h, &sb)?, 6);
    assert_eq!(
        h.core
            .next_batch(&sa, h.now, |_| ())
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: sa.stream()
        }
    );
    Ok(())
}

#[when(
    "a statement is prepared, the catalog of its namespace changes, and the statement is executed"
)]
fn stale(w: &mut Steps) -> TestResult {
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
            PatternSlot::Parameter(0),
            PatternSlot::Bound(name),
            PatternSlot::Unbound,
        ),
        vec![ColumnType::Node],
        Vec::new(),
    );
    let p = h.core.prepare(&prepare, h.now)?;
    let statement = prepared(h.settle(p)?)?;
    h.server.set_schema(2);
    let tx = begin(&mut h, true)?;
    let mut prepare_terms = TermTable::new();
    let a = prepare_terms.iri("http://ex.org/a");
    let stream = h.core.execute(
        &tx,
        &Execute::new(
            statement.id(),
            tx.stream(),
            0,
            prepare_terms,
            vec![a],
            Vec::new(),
        ),
        h.now,
    )?;
    let mut refused = None;
    h.until(|h| {
        refused = h.core.next_batch(&stream, h.now, |_| ()).err();
        refused.is_some()
    })?;
    w.refusal = refused.map(|e| e.is_stale());
    let preparing = h.core.prepare(&prepare, h.now)?;
    let again = h.settle(preparing)?;
    w.outcome = match again {
        Ok(Reply::Prepared(s)) => vec![s.schema()],
        _ => Vec::new(),
    };
    Ok(())
}

#[then("the execution is refused as stale, and preparing it again succeeds")]
fn refused_stale(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(true));
    assert_eq!(w.outcome, vec![2]);
    Ok(())
}

#[when("a batch of 1,000 rows names the same IRI in every row")]
fn repeated(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h, true)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.core.transmit().is_some() {}
    let iri = ResultTerm::new(EncodedTerm::uid(42)).with_text("http://ex.org/same");
    let mut batch = ResultBatch::new(1);
    for _ in 0..1_000 {
        batch.push_row(&[Some(&iri)])?;
    }
    let frame = batch.frame(
        Header::new(CLUSTER, Command::Result)
            .with_stream(stream.stream())
            .with_flags(FLAG_LAST),
    )?;
    let frame = h.numbered(&frame)?;
    h.deliver(&frame)??;
    let polled = h.core.next_batch(&stream, h.now, |v| {
        let same = (0..v.rows()).all(|r| {
            v.cell(0, r).and_then(|c| v.term(c)).and_then(|t| t.text())
                == Some("http://ex.org/same")
        });
        (v.rows(), v.term_count(), same)
    });
    assert!(matches!(polled, Ok(BatchPoll::Batch(_))), "the batch");
    let Ok(BatchPoll::Batch((rows, terms, same))) = polled else {
        return Ok(());
    };

    assert!(same && rows == 1_000);
    w.terms = terms;
    Ok(())
}

#[then("its term table holds that IRI once")]
fn once(w: &mut Steps) -> TestResult {
    assert_eq!(w.terms, 1);
    Ok(())
}

/// Every Commit frame the server received: its session and request.
fn commits(h: &Harness) -> Vec<(u64, u64)> {
    h.server
        .received()
        .iter()
        .filter_map(|f| {
            let view = FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE).ok()?;
            CommitView::from_frame(&view)
                .ok()
                .map(|c| (c.session(), c.request()))
        })
        .collect()
}

#[when(
    "the connection drops before a commit reaches the commit step, and the client retries it through its session"
)]
fn lost_before_the_step(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    h.server.lose_next_commit();
    let p = h.core.commit(tx, session, h.now)?;
    let refused = h.settle(p)?.err().context("an error")?;
    w.refusal = Some(
        matches!(
            refused.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::NotCommitted,
                ..
            }
        ) && refused.retryable(),
    );
    let next_tx = begin(&mut h, false)?;
    let committing = h.core.commit(next_tx, session, h.now)?;
    w.outcome = match h.settle(committing)? {
        Ok(Reply::Committed(c)) => vec![c.request(), h.server.applied()],
        _ => Vec::new(),
    };
    Ok(())
}

#[then(
    "the retry is refused with code 18, the client runs its transaction again, and it commits once"
)]
fn ran_again(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(true));
    assert_eq!(w.outcome, vec![1, 1], "request 1, applied once");
    Ok(())
}

#[when("the client retries a commit whose session the table evicted")]
fn evicted(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, session, h.now)?;
    h.until(|h| h.connections == 2)?;
    h.server.evict(session.number());
    let refused = h.settle(p)?.err().context("an error")?;
    w.refusal = Some(
        refused.0
            == ClientErrorEnum::OutcomeUnknown {
                session: session.number(),
                request: 1,
            },
    );
    w.outcome = vec![commits(&h).len() as u64];
    Ok(())
}

#[then(
    "the retry is refused with code 12, and the client reports the commit's outcome as unknown without retrying"
)]
fn unknown(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(true));
    assert_eq!(w.outcome, vec![2], "the first send and one retry");
    Ok(())
}

#[when("the network delivers one of the client's frames twice")]
fn repeated_frame(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    let p = h.core.commit(tx, session, h.now)?;
    h.send_twice();
    let c = committed(h.settle(p)?)?;
    w.outcome = vec![
        c.request(),
        h.server.applied(),
        u64::from(h.connections),
        commits(&h).len() as u64,
    ];
    Ok(())
}

#[then(
    "the server refuses the second as out of sequence and closes the connection, and the client reconnects and retries its commit through its session, which commits once"
)]
fn refused_and_retried(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.outcome,
        vec![1, 1, 2, 3],
        "request 1 applied once, a second connection, the commit, its repeat and the retry"
    );
    Ok(())
}

#[when("a transaction runs two reads at once, each on a stream of its own naming the transaction")]
fn two_reads(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server().with_results(4))?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    let first = h.core.read(&tx, &read(&tx), h.now)?;
    let second = h.core.read(&tx, &read(&tx), h.now)?;
    assert!(first.stream() != tx.stream() && second.stream() != tx.stream());
    w.outcome = vec![rest(&mut h, &first)? as u64, rest(&mut h, &second)? as u64];
    w.harness = Some(h);
    w.transaction = Some(tx);
    w.session = Some(session);
    Ok(())
}

#[then(
    "both stream their results at the transaction's snapshot, each with its own credit, and the transaction then commits on its own stream"
)]
fn both_streamed(w: &mut Steps) -> TestResult {
    assert_eq!(w.outcome, vec![4, 4]);
    let h = w.harness.as_mut().context("a harness")?;
    let tx = w.transaction.context("a transaction")?;
    let session = w.session.context("a session")?;
    let stream = tx.stream();
    let p = h.core.commit(tx, session, h.now)?;
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    let last = h.server.received().last().context("a frame received")?;
    assert_eq!(
        ultragraphdb_protocol_wire::HeaderView::new(last)?.stream(),
        stream,
        "the commit went on the transaction's stream"
    );
    Ok(())
}

#[when(
    "a client cancels one of its transaction's reads mid-result and then commits the transaction"
)]
fn cancel_a_read(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server().with_results(6))?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    h.until(|h| {
        matches!(
            h.core.next_batch(&stream, h.now, |_| ()),
            Ok(BatchPoll::Batch(()))
        )
    })?;
    h.core.cancel(stream);
    let p = h.core.commit(tx, session, h.now)?;
    w.refusal = Some(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    w.harness = Some(h);
    w.stream = Some(stream);
    Ok(())
}

#[then("no further frame of the read arrives, and the commit succeeds")]
fn read_cancelled(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(true), "the commit succeeds");
    let h = w.harness.as_mut().context("a harness")?;
    let stream = w.stream.context("a stream")?;
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

#[when("a transaction commits while one of its reads has frames left to send")]
fn commit_with_a_read_open(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server().with_results(10))?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let p = h.core.commit(tx, session, h.now)?;
    w.refusal = Some(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    w.harness = Some(h);
    w.stream = Some(stream);
    Ok(())
}

#[then("the read's stream sends no further frame, and the commit's reply arrives")]
fn read_ended(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(true), "the commit's reply arrived");
    let h = w.harness.as_mut().context("a harness")?;
    let stream = w.stream.context("a stream")?;
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

#[when(
    "a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the principal's password"
)]
fn scram(w: &mut Steps) -> TestResult {
    let mut h = Harness::authenticated(TestServer::new(CLUSTER, scram_welcome()))?;
    let p = h.core.register(h.now)?;
    w.refusal = Some(matches!(h.settle(p)?, Ok(Reply::Registered(_))));
    w.outcome = vec![
        h.core.principal().unwrap_or(0),
        h.server.principal().unwrap_or(0),
    ];
    Ok(())
}

#[then("the server's final Authenticate names the principal, and the client's requests are served")]
fn served(w: &mut Steps) -> TestResult {
    assert_eq!(w.outcome, vec![TEST_PRINCIPAL, TEST_PRINCIPAL]);
    assert_eq!(w.refusal, Some(true));
    Ok(())
}

#[when(
    "a connection whose credential expires runs a new exchange as the same principal before the expiry, while one of its reads streams"
)]
fn renewal(w: &mut Steps) -> TestResult {
    let life = Duration::from_secs(120);
    let mut h = Harness::authenticated(
        TestServer::new(CLUSTER, scram_welcome())
            .with_credential_life(life)
            .with_results(6),
    )?;
    let first = h.core.expires();
    let p = h.core.begin(0, true, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    h.until(|h| h.server.received().len() >= 5)?;
    while h.step()? {}
    h.now = h.core.next_deadline().context("the renewal")?;
    h.core.handle_timeout(h.now);
    h.until(|h| h.core.expires() > first)?;
    h.now += life / 2;
    w.outcome = vec![
        rest(&mut h, &stream)? as u64,
        u64::from(h.connections),
        h.core.principal().unwrap_or(0),
    ];
    // A second past the first expiry, both sides' deadlines checked, a request is still served on this connection.
    h.now += Duration::from_secs(1);
    assert!(h.unix() > first.unwrap_or(0), "past the first expiry");
    h.server.handle_timeout(h.now);
    h.core.handle_timeout(h.now);
    let registering = h.core.register(h.now)?;
    w.outcome.push(u64::from(matches!(
        h.settle(registering)?,
        Ok(Reply::Registered(_))
    )));
    w.outcome.push(u64::from(h.connections));
    Ok(())
}

#[then("the read continues, and the connection serves requests past the first expiry")]
fn renewed(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.outcome,
        vec![6, 1, TEST_PRINCIPAL, 1, 1],
        "six batches, one connection, the principal; past the first expiry a Register served, on that connection"
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
