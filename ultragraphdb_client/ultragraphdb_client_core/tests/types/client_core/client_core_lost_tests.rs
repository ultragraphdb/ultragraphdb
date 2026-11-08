/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A lost connection, as its requests and reads answer it: `ConnectionLost` carrying what gave the connection up (the
//! framer's refusal, the rule a frame broke, the server's closing Error, or nothing for a plain loss), and a read cut
//! short before its last frame never finished. `PROTOCOL.md` §11.4: a closing connection aborts its transactions, so
//! nothing of them is retried.

use std::error::Error;
use std::time::Duration;

use ultragraphdb_client_core::{
    BatchPoll, ClientConfig, ClientCore, ClientError, Pending, ResultStream, Transaction,
};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Anonymous, BufferPool, Framer, TransportError};
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, Command, ErrorCode, ErrorReply, FrameView, Header, MAX_FRAME_SIZE,
    PatternSlot, QuadPattern, Read, ResultBatch, TermTable,
};

use crate::types::client_core::client_core_conformance_tests::{
    golden, numbered, restream, stream_of,
};
use crate::utils::harness::{CLUSTER, RELEASE, UNIX, begun, frame_through, start};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// A core welcomed by the golden Welcome, its connection's framer, and the frames the server sent.
struct Conn {
    core: ClientCore<Anonymous>,
    framer: Framer,
    pool: BufferPool,
    sent: u128,
}

impl Conn {
    fn new() -> TestResult<Self> {
        let config = ClientConfig::new(CLUSTER, "ultragraph-test")
            .with_release(RELEASE)
            .with_timeout(Duration::from_secs(1));
        let mut conn = Self {
            core: ClientCore::new(config, Anonymous),
            framer: Framer::new(CLUSTER, MAX_FRAME_SIZE),
            pool: BufferPool::new(16, 1 << 20),
            sent: 0,
        };
        conn.connect()?;
        Ok(conn)
    }

    /// A new connection, welcomed.
    fn connect(&mut self) -> TestResult {
        self.core.connect(start(), UNIX)?;
        self.framer = Framer::new(CLUSTER, MAX_FRAME_SIZE);
        self.sent = 0;
        while self.core.transmit().is_some() {}
        self.deliver(&golden("welcome_no_auth")?)??;
        assert!(self.core.is_ready());
        Ok(())
    }

    /// `frame` numbered as the connection's next and received: the core's answer, or the test's failure when `frame`
    /// is no whole frame.
    fn deliver(&mut self, frame: &[u8]) -> TestResult<Result<(), ClientError>> {
        self.sent += 1;
        let stamped = numbered(frame, self.sent);
        let received =
            frame_through(&mut self.framer, &self.pool, &stamped).context("a whole frame")?;
        Ok(self.core.receive(received, start()))
    }

    /// The stream of the last frame the core queued, every queued frame given up as sent.
    fn stream(&mut self) -> TestResult<u64> {
        let mut stream = 0;
        while let Some(frame) = self.core.transmit() {
            stream = stream_of(&frame)?;
        }
        Ok(stream)
    }

    fn begin(&mut self, read_only: bool) -> TestResult<Transaction> {
        let p = self.core.begin(0, read_only, start())?;
        let stream = self.stream()?;
        let frame = Begin::new(0, if read_only { 6 } else { 0 }, read_only)
            .frame(Header::new(CLUSTER, Command::Begin).with_stream(stream))?;
        self.deliver(&frame)??;
        begun(self.core.take_reply(p).context("an answer")?)
    }

    fn read(&mut self, tx: &Transaction) -> TestResult<ResultStream> {
        let mut terms = TermTable::new();
        let a = terms.iri("http://ex.org/a");
        let pattern = QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(a),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        let stream = self.core.read(
            tx,
            &Read::new(tx.stream(), 0, terms, pattern, Vec::new()),
            start(),
        )?;
        let _stream: u64 = self.stream()?;
        Ok(stream)
    }

    /// The golden Result on `stream`: flagged last, or not.
    fn result(&mut self, stream: &ResultStream, last: bool) -> TestResult {
        let name = if last { "result" } else { "result_more" };
        let frame = restream(&golden(name)?, stream.stream());
        self.deliver(&frame)?.context("the result taken")
    }

    /// The next batch's row count (the golden Result's 3 rows), or why there is none.
    fn batch(&mut self, stream: &ResultStream) -> Result<BatchPoll<u32>, ClientError> {
        self.core.next_batch(stream, start(), |v| v.rows())
    }

    /// A Register waiting for its reply.
    fn register(&mut self) -> TestResult<Pending> {
        let p = self.core.register(start())?;
        let _stream: u64 = self.stream()?;
        Ok(p)
    }
}

/// A read with two batches arrived of a stream not yet ended, and a Register waiting.
fn reading() -> TestResult<(Conn, ResultStream, Pending)> {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let stream = c.read(&tx)?;
    c.result(&stream, false)?;
    c.result(&stream, false)?;
    // The golden Result's three rows (`generate_bodies.py`).
    assert!(matches!(c.batch(&stream), Ok(BatchPoll::Batch(3))));
    let p = c.register()?;
    Ok((c, stream, p))
}

/// The batch arrived before the loss is still read, then the read fails as lost, once: it is not finished.
fn assert_cut_short(c: &mut Conn, stream: &ResultStream, lost: &ClientError) -> TestResult {
    assert!(!c.core.is_connected());
    assert_eq!(
        c.batch(stream).err().context("an error")?,
        *lost,
        "the read is cut short"
    );
    assert_eq!(
        c.batch(stream).err().context("an error")?,
        ClientError::stream_finished(stream.stream()),
        "given once, as a refusal ends a read"
    );
    Ok(())
}

#[test]
fn test_a_read_cut_short_by_a_plain_loss_fails_as_lost_without_a_cause() -> TestResult {
    let (mut c, stream, p) = reading()?;
    c.core.disconnected(TransportError::closed(), start());
    let lost = ClientError::connection_lost(None);
    assert_eq!(c.core.take_reply(p), Some(Err(lost.clone())));
    assert_cut_short(&mut c, &stream, &lost)?;
    Ok(())
}

#[test]
fn test_a_framer_refusal_is_the_cause_of_every_loss() -> TestResult {
    let (mut c, stream, p) = reading()?;
    // A header the framer refuses: of another cluster (`PROTOCOL.md` §2.5 step 2).
    let refusal = TransportError::protocol(ProtocolError::wrong_cluster(CLUSTER, 7));
    c.core.disconnected(refusal.clone(), start());
    let lost = ClientError::connection_lost(Some(ClientError::transport(refusal)));
    assert_eq!(c.core.take_reply(p), Some(Err(lost.clone())));
    assert_cut_short(&mut c, &stream, &lost)?;
    Ok(())
}

#[test]
fn test_a_truncated_frame_is_the_cause_of_every_loss() -> TestResult {
    let (mut c, stream, p) = reading()?;
    let cut = TransportError::truncated(100, 256);
    c.core.disconnected(cut.clone(), start());
    let lost = ClientError::connection_lost(Some(ClientError::transport(cut)));
    assert_eq!(c.core.take_reply(p), Some(Err(lost.clone())));
    assert_cut_short(&mut c, &stream, &lost)?;
    Ok(())
}

#[test]
fn test_the_rule_a_frame_broke_is_the_cause_of_every_loss() -> TestResult {
    let (mut c, stream, p) = reading()?;
    // The golden Error on the Register's stream (3: the transaction's 1 and the read's 2 before it), a body byte flipped:
    // its chunk no longer verifies, a rule of the frame's, past the framer's header checks.
    let mut bad = numbered(&restream(&golden("error")?, 3), c.sent + 1);
    bad[256] ^= 1;
    let rule = FrameView::validate(&bad, CLUSTER, MAX_FRAME_SIZE)
        .err()
        .context("refused by a rule")?;
    let refused = c.deliver(&bad[..])?.err().context("refused")?;
    assert_eq!(refused, ClientError::protocol(rule.clone()));
    let lost = ClientError::connection_lost(Some(ClientError::protocol(rule)));
    assert_eq!(c.core.take_reply(p), Some(Err(lost.clone())));
    assert_cut_short(&mut c, &stream, &lost)?;
    Ok(())
}

#[test]
fn test_the_servers_closing_error_is_the_cause_of_every_loss() -> TestResult {
    let (mut c, stream, p) = reading()?;
    let closing = ErrorReply::new(ErrorCode::Protocol, "bye")
        .frame(Header::new(CLUSTER, Command::Error).with_flags(1))?;
    assert_eq!(
        c.deliver(&closing)?.err().context("an error")?,
        ClientError::closed(ErrorCode::Protocol, "bye")
    );
    let lost = ClientError::connection_lost(Some(ClientError::closed(ErrorCode::Protocol, "bye")));
    assert_eq!(c.core.take_reply(p), Some(Err(lost.clone())));
    assert_cut_short(&mut c, &stream, &lost)?;
    Ok(())
}

#[test]
fn test_a_read_cut_short_answers_its_loss_after_the_next_connection_opened() -> TestResult {
    let (mut c, stream, _) = reading()?;
    c.core.disconnected(TransportError::closed(), start());
    c.connect()?;
    assert_cut_short_connected(&mut c, &stream, &ClientError::connection_lost(None))?;
    Ok(())
}

fn assert_cut_short_connected(
    c: &mut Conn,
    stream: &ResultStream,
    lost: &ClientError,
) -> TestResult {
    assert_eq!(c.batch(stream).err().context("an error")?, *lost);
    assert_eq!(
        c.batch(stream).err().context("an error")?,
        ClientError::stream_finished(stream.stream())
    );
    assert!(c.core.is_ready(), "the new connection is not given up");
    Ok(())
}

#[test]
fn test_a_read_timed_out_answers_as_its_requests_do() -> TestResult {
    let (mut c, stream, p) = reading()?;
    let at = start() + Duration::from_secs(5);
    c.core.handle_timeout(at);
    assert_eq!(c.core.take_reply(p), Some(Err(ClientError::timed_out())));
    assert_cut_short(&mut c, &stream, &ClientError::timed_out())?;
    Ok(())
}

#[test]
fn test_a_read_whose_last_frame_arrived_reads_to_its_end_after_the_loss() -> TestResult {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let stream = c.read(&tx)?;
    c.result(&stream, false)?;
    c.result(&stream, true)?;
    c.core.disconnected(TransportError::closed(), start());
    assert!(matches!(c.batch(&stream), Ok(BatchPoll::Batch(3))));
    assert!(matches!(c.batch(&stream), Ok(BatchPoll::Batch(3))));
    assert!(matches!(c.batch(&stream), Ok(BatchPoll::Done)));
    Ok(())
}

#[test]
fn test_a_read_that_ended_normally_answers_stream_finished() -> TestResult {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let stream = c.read(&tx)?;
    c.result(&stream, false)?;
    c.core.abort(tx)?;
    assert_eq!(
        c.batch(&stream).err().context("an error")?,
        ClientError::stream_finished(stream.stream()),
        "its transaction ended"
    );
    c.core.disconnected(TransportError::closed(), start());
    assert_eq!(
        c.batch(&stream).err().context("an error")?,
        ClientError::stream_finished(stream.stream()),
        "ended before the loss"
    );
    Ok(())
}

#[test]
fn test_a_cancelled_read_is_not_lost_with_its_connection() -> TestResult {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let stream = c.read(&tx)?;
    c.core.cancel(stream.clone());
    c.core.disconnected(TransportError::closed(), start());
    assert_eq!(
        c.batch(&stream).err().context("an error")?,
        ClientError::stream_finished(stream.stream())
    );
    Ok(())
}

#[test]
fn test_the_cause_is_the_losss_source() -> TestResult {
    let (mut c, _, p) = reading()?;
    let cut = TransportError::truncated(100, 256);
    c.core.disconnected(cut.clone(), start());
    let lost = c
        .core
        .take_reply(p)
        .context("an answer")?
        .err()
        .context("an error")?;
    let source = lost.source().context("a source")?;
    assert_eq!(
        source.downcast_ref::<ClientError>(),
        Some(&ClientError::transport(cut))
    );
    Ok(())
}

/// A read whose last frame, a refusal, had arrived when its connection was lost reads its batches, then the refusal,
/// once, and is finished.
#[test]
fn test_a_kept_read_ending_in_a_refusal_answers_it_after_the_loss() -> TestResult {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let stream = c.read(&tx)?;
    c.result(&stream, false)?;
    let refusal = ErrorReply::new(ErrorCode::OperationFailed, "the read failed")
        .with_detail(4)
        .frame(
            Header::new(CLUSTER, Command::Error)
                .with_stream(stream.stream())
                .with_flags(ultragraphdb_protocol_wire::FLAG_LAST),
        )?;
    c.deliver(&refusal)??;
    c.core.disconnected(TransportError::closed(), start());
    assert_eq!(c.batch(&stream), Ok(BatchPoll::Batch(3)));
    assert_eq!(
        c.batch(&stream),
        Err(
            ClientError::refused(ErrorCode::OperationFailed, 4, "the read failed", None)
                .of_request(ultragraphdb_protocol_wire::RefusedRequest::Read)
        )
    );
    assert_eq!(
        c.batch(&stream),
        Err(ClientError::stream_finished(stream.stream()))
    );
    Ok(())
}

/// A read whose last frame, kept after the loss, breaks its command's layout answers the rule it broke, once.
#[test]
fn test_a_kept_read_ending_in_a_broken_frame_answers_its_rule_after_the_loss() -> TestResult {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let stream = c.read(&tx)?;
    // A Result flagged last, a body byte flipped: its chunk is checked when it is read.
    let mut broken = ResultBatch::new(2).frame(
        Header::new(CLUSTER, Command::Result)
            .with_stream(stream.stream())
            .with_flags(ultragraphdb_protocol_wire::FLAG_LAST),
    )?;
    broken[300] ^= 1;
    c.deliver(&broken)??;
    c.core.disconnected(TransportError::closed(), start());
    let refused = c.batch(&stream).err().context("an error")?;
    assert!(
        matches!(
            &refused.0,
            ultragraphdb_client_core::ClientErrorEnum::Protocol { .. }
        ),
        "{refused:?}"
    );
    assert_eq!(
        c.batch(&stream),
        Err(ClientError::stream_finished(stream.stream()))
    );
    Ok(())
}

/// A read kept, or cut short, by a lost connection and then cancelled is forgotten: nothing is sent for it, and it is
/// finished.
#[test]
fn test_a_kept_or_lost_read_cancelled_is_forgotten() -> TestResult {
    for kept in [false, true] {
        let mut c = Conn::new()?;
        let tx = c.begin(true)?;
        let stream = c.read(&tx)?;
        c.result(&stream, kept)?;
        c.core.disconnected(TransportError::closed(), start());
        c.connect()?;
        c.core.cancel(stream.clone());
        assert_eq!(c.core.transmit(), None, "kept {kept}: no Cancel");
        assert_eq!(
            c.batch(&stream),
            Err(ClientError::stream_finished(stream.stream())),
            "kept {kept}"
        );
    }
    Ok(())
}

/// A read of an earlier connection, neither kept nor lost, cancelled on the next: nothing is sent, and the stream of
/// that number on the new connection is left alone.
#[test]
fn test_a_read_of_an_earlier_connection_cancelled_sends_nothing() -> TestResult {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let stream = c.read(&tx)?;
    c.core.cancel(stream.clone());
    let _cancel: u64 = c.stream()?;
    c.core.disconnected(TransportError::closed(), start());
    c.connect()?;
    let again = c.begin(true)?;
    let renewed = c.read(&again)?;
    assert_eq!(renewed.stream(), stream.stream(), "the same number again");
    c.core.cancel(stream.clone());
    assert_eq!(c.core.transmit(), None, "no Cancel for the earlier read");
    c.result(&renewed, true)?;
    assert_eq!(c.batch(&renewed), Ok(BatchPoll::Batch(3)));
    Ok(())
}

/// The core's state in brief counts every read a lost connection left it remembering: one whose last frame had arrived,
/// kept to be read to its end, and one cut short, kept to answer its loss.
#[test]
fn test_debug_counts_the_kept_and_the_cut_short_reads() -> TestResult {
    let mut c = Conn::new()?;
    let tx = c.begin(true)?;
    let kept = c.read(&tx)?;
    let cut = c.read(&tx)?;
    c.result(&kept, true)?;
    c.result(&cut, false)?;
    c.core.disconnected(TransportError::closed(), start());
    let state = format!("{:?}", c.core);
    assert!(state.contains("remembered_reads: 2,"), "{state}");
    Ok(())
}
