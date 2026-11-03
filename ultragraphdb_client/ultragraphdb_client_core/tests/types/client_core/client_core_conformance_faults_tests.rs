/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conformance suite under the simulator's faults: every golden reply of `ultragraphdb_protocol_wire` crosses a
//! simulated link that drops, delays, duplicates or splits it, behind it a frame of a stream nothing opened, and the
//! client's framer reads what arrives as a shell does, the bytes it asks for and no more. What the client must make
//! of the arrivals is the connection's rule alone (`PROTOCOL.md` §2.5 step 13): each frame numbered one above the last
//! is taken, so a golden reply arriving at its place is read as its golden values; the first frame that is not is
//! refused with `SequenceOrder` naming both numbers, and the connection is given up. Which arrivals there were comes
//! from the network's own counters around each send, never from the client.

use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use ultragraphdb_client_core::{
    BatchPoll, ClientConfig, ClientCore, ClientError, ClientErrorEnum, Pending, Reply, ResultStream,
};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_scram::{ScramError, ScramLogin};
use ultragraphdb_protocol_transport::{
    Anonymous, BufferPool, FrameStep, Framer, Login, NetworkFaults, ReceivedFrame, SimNetwork,
    SimStream, TransportError,
};
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, ColumnType, Command, DescribeView, DropCommand, ErrorCode, FrameView,
    Header, MAX_FRAME_SIZE, OperationResult, PatternSlot, PrepareView, QuadPattern, Read,
    RequestTerm, TermTable,
};

use crate::types::client_core::client_core_conformance_tests::{
    golden, numbered, restream, stream_of, view,
};
use crate::utils::harness::{
    CLUSTER, RELEASE, begun, committed, described, dropped, frame_through, prepared, registered,
    start,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// A stream no request opens: a frame on it is late and dropped once taken, so it shows only whether the frame before
/// it was taken at its place.
const PROBE: u64 = 1_000;
/// The wall clock the connections open at: 2026-12-31T00:00:00Z, a day before the golden credential's expiry of
/// 1,798,761,600,000,000 µs (2027-01-01T00:00:00Z, `generate_bodies.py`'s Authenticate step 3).
const UNIX: u64 = 1_798_675_200_000_000;
/// The seeds of each fault class and golden frame.
const SEEDS: u64 = 8;

/// The fault classes: each fault alone at a rate that leaves some frames untouched, then all four together.
fn classes() -> [(&'static str, NetworkFaults); 5] {
    [
        ("drop", NetworkFaults::none().with_drop(350_000)),
        ("duplicate", NetworkFaults::none().with_duplicate(350_000)),
        ("delay", NetworkFaults::none().with_delay(600_000, 4)),
        ("split", NetworkFaults::none().with_split(700_000)),
        (
            "all",
            NetworkFaults::none()
                .with_drop(200_000)
                .with_duplicate(200_000)
                .with_delay(400_000, 3)
                .with_split(500_000),
        ),
    ]
}

/// The client's configuration of the golden frames: their cluster, release and client name (`generate_bodies.py`'s
/// Hello).
fn config() -> ClientConfig {
    ClientConfig::new(CLUSTER, "ultragraph-test")
        .with_release(RELEASE)
        .with_timeout(Duration::from_secs(1))
}

/// RFC 7677 §3's client nonce, the one the golden Authenticate exchange was made with.
fn rfc_nonce() -> Result<String, ScramError> {
    Ok("rOprNGfwEbeRWgbNEkqO".to_string())
}

type RfcLogin = ScramLogin<fn() -> Result<String, ScramError>>;

/// RFC 7677 §3's user and password.
fn rfc_login() -> TestResult<RfcLogin> {
    ScramLogin::new(
        "user",
        "pencil",
        rfc_nonce as fn() -> Result<String, ScramError>,
    )
    .context("RFC 7677's user and password prepare")
}

/// The copies of `frame` the network delivers, sent from `end`: none when dropped, two when duplicated, read from the
/// network's counters around the send.
fn send(network: &SimNetwork, end: &SimStream, frame: Vec<u8>) -> usize {
    let before = network.stats();
    end.send(frame);
    let after = network.stats();
    if after.dropped() > before.dropped() {
        0
    } else if after.duplicated() > before.duplicated() {
        2
    } else {
        1
    }
}

/// The next frame arrived at `end`, read as a shell reads it: the bytes `framer` asks for, into a buffer of `pool`.
/// `None` when nothing more has arrived between frames; a frame cut short where nothing more arrives fails the test,
/// as the network never loses part of a message.
fn arrive(
    end: &SimStream,
    framer: &mut Framer,
    pool: &BufferPool,
) -> TestResult<Option<Result<ReceivedFrame, TransportError>>> {
    let mut cx = Context::from_waker(Waker::noop());
    let mut buffer = pool.acquire().context("a buffer")?;
    framer.reset();
    loop {
        let mut bytes = buffer.take();
        while bytes.len() < framer.want() {
            match end.poll_chunk(&mut cx, framer.want() - bytes.len()) {
                Poll::Ready(Some(chunk)) => bytes.extend_from_slice(&chunk),
                Poll::Ready(None) => {
                    let received = bytes.len();
                    buffer.restore(bytes);
                    return Ok(Some(Err(framer.closed(received))));
                }
                Poll::Pending => {
                    assert!(bytes.is_empty(), "a frame cut at {} bytes", bytes.len());
                    return Ok(None);
                }
            }
        }
        buffer.restore(bytes);
        match framer.advance(&buffer) {
            Ok(FrameStep::Read(_)) => {}
            Ok(FrameStep::Frame) => return Ok(Some(Ok(framer.finish(buffer)))),
            Err(refusal) => return Ok(Some(Err(refusal))),
        }
    }
}

/// The connection's rule over `arrivals`, the sequences in the order they arrive, the first due at `first`: how many
/// are taken, and the refusal of the one after them, if any.
fn rule(first: u128, arrivals: &[u128]) -> (usize, Option<ProtocolError>) {
    for (taken, (&found, expected)) in arrivals.iter().zip(first..).enumerate() {
        if found != expected {
            return (taken, Some(ProtocolError::sequence_order(expected, found)));
        }
    }
    (arrivals.len(), None)
}

/// What the client waits for when the golden frame crosses.
enum Wait {
    /// A reply; `retried` for a commit or a drop, which a lost connection sends again rather than fails.
    Reply { pending: Pending, retried: bool },
    /// A result's batch.
    Batch(ResultStream),
    /// The handshake's Welcome, or the Authenticate exchange's next message.
    Connection,
}

/// What became of the golden frames, and what the networks did to the frames they carried.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    delivered: u32,
    refused: u32,
    lost: u32,
    dropped: u64,
    duplicated: u64,
    delayed: u64,
    split: u64,
}

/// The client's core of the golden frames' cluster and release, its connection's framer, and the frames the server
/// sent on it: the frames before the one under test are handed over directly, numbered as the connection's next.
struct Peer<L: Login> {
    core: ClientCore<L>,
    framer: Framer,
    pool: BufferPool,
    sent: u128,
    now: Instant,
}

impl Peer<Anonymous> {
    /// A core connected to a server asking for no authentication, its golden Welcome taken.
    fn welcomed() -> TestResult<Self> {
        let mut peer = Self::connecting(Anonymous)?;
        peer.direct(&golden("welcome_no_auth")?)?;
        assert!(peer.core.is_ready());
        Ok(peer)
    }
}

impl<L: Login> Peer<L> {
    /// A core that sent its Hello and waits for the Welcome.
    fn connecting(login: L) -> TestResult<Self> {
        let now = start();
        let mut core = ClientCore::new(config(), login);
        core.connect(now, UNIX).context("the core connects")?;
        let hello = core.transmit().context("the Hello")?;
        // The Hello is golden byte for byte (`generate_bodies.py`: the client's first frame, sequence 1).
        assert_eq!(hello, golden("hello")?);
        Ok(Self {
            core,
            framer: Framer::new(CLUSTER, MAX_FRAME_SIZE),
            pool: BufferPool::new(8, 1 << 20),
            sent: 0,
            now,
        })
    }

    /// `frame` numbered as the connection's next and taken.
    fn direct(&mut self, frame: &[u8]) -> TestResult {
        self.sent += 1;
        let stamped = numbered(frame, self.sent);
        let received =
            frame_through(&mut self.framer, &self.pool, &stamped).context("a whole frame")?;
        self.core.receive(received, self.now).context("taken")
    }

    /// The stream of the last request the core queued, every queued frame given up as sent.
    fn request_stream(&mut self) -> TestResult<u64> {
        let mut stream = None;
        while let Some(frame) = self.core.transmit() {
            stream = Some(stream_of(&frame)?);
        }
        stream.context("a request")
    }

    /// A transaction begun on its stream, its Begin answered directly.
    fn begin(&mut self, read_only: bool) -> TestResult<ultragraphdb_client_core::Transaction> {
        let p = self.core.begin(0, read_only, self.now)?;
        let stream = self.request_stream()?;
        let frame = Begin::new(0, 5, read_only)
            .frame(Header::new(CLUSTER, Command::Begin).with_stream(stream))?;
        self.direct(&frame)?;
        begun(self.core.take_reply(p).context("an answer")?)
    }

    /// `frame`, the golden frame moved to the stream it answers, crosses a link of `faults` seeded `seed`, behind it
    /// the probe; what arrives is read in order, `check` reads the golden values the moment the golden frame is taken.
    fn cross(
        &mut self,
        faults: NetworkFaults,
        seed: u64,
        frame: &[u8],
        wait: &Wait,
        check: impl Fn(&mut Self, &Wait) -> TestResult,
        tally: &mut Tally,
    ) -> TestResult {
        let network = SimNetwork::new(seed, faults);
        let (client, server) = network.connect();
        let at = self.sent + 1;
        let sent = [
            numbered(frame, at),
            numbered(&restream(frame, PROBE), at + 1),
        ];
        let arrivals: Vec<u128> = sent
            .iter()
            .zip([at, at + 1])
            .flat_map(|(copy, sequence)| {
                std::iter::repeat_n(sequence, send(&network, &server, copy.clone()))
            })
            .collect();
        self.sent += 2;
        let (taken, refusal) = rule(at, &arrivals);
        let mut delivered = false;
        for (i, &sequence) in arrivals.iter().enumerate() {
            let received = arrive(&client, &mut self.framer, &self.pool)?
                .context("arrived")?
                .context("framed whole")?;
            // Split and delayed, the frame arrives as it was sent.
            assert_eq!(received.bytes(), &sent[(sequence - at) as usize][..]);
            let outcome = self.core.receive(received, self.now);
            if i < taken {
                assert_eq!(outcome, Ok(()), "arrival {i} of {arrivals:?}");
                if sequence == at {
                    delivered = true;
                    check(self, wait)?;
                }
            } else {
                let rule = refusal.clone().context("the refusal")?;
                assert_eq!(outcome, Err(ClientError::protocol(rule)), "{arrivals:?}");
                assert!(!self.core.is_connected(), "the connection is given up");
                break;
            }
        }
        if refusal.is_none() {
            assert!(
                arrive(&client, &mut self.framer, &self.pool)?.is_none(),
                "nothing more arrived"
            );
        }
        let stats = network.stats();
        tally.dropped += stats.dropped();
        tally.duplicated += stats.duplicated();
        tally.delayed += stats.delayed();
        tally.split += stats.split();
        if delivered {
            tally.delivered += 1;
        } else if refusal.is_some() {
            tally.refused += 1;
            let rule = ClientError::protocol(refusal.context("the refusal")?);
            self.lost(wait, ClientError::connection_lost(Some(rule)))?;
        } else {
            tally.lost += 1;
            self.now += config().timeout();
            self.core.handle_timeout(self.now);
            self.lost(wait, ClientError::timed_out())?;
        }
        Ok(())
    }

    /// The golden frame never taken: the connection given up for `reason`, what waited fails with it, a read cut short
    /// too, a commit or a drop waits for the next connection to be sent again, a handshake is down.
    fn lost(&mut self, wait: &Wait, reason: ClientError) -> TestResult {
        assert!(!self.core.is_connected());
        match wait {
            Wait::Reply {
                pending,
                retried: false,
            } => assert_eq!(self.core.take_reply(*pending), Some(Err(reason))),
            Wait::Reply {
                pending,
                retried: true,
            } => {
                assert_eq!(self.core.take_reply(*pending), None);
                assert!(self.core.needs_connection(), "sent again on the next");
            }
            Wait::Batch(stream) => assert_eq!(
                self.core
                    .next_batch(stream, self.now, |_| ())
                    .err()
                    .context("an error")?,
                reason
            ),
            Wait::Connection => assert!(!self.core.is_ready()),
        }
        Ok(())
    }
}

/// One golden reply: how the client comes to wait for it, on which stream, and its golden values.
struct Case {
    name: &'static str,
    setup: fn(&mut Peer<Anonymous>) -> TestResult<(Wait, u64)>,
    check: fn(&mut Peer<Anonymous>, &Wait) -> TestResult,
}

/// The request `wait` waits for, a reply's.
fn pending(wait: &Wait) -> TestResult<Pending> {
    if let Wait::Reply { pending, .. } = wait {
        Some(*pending)
    } else {
        None
    }
    .context("a reply")
}

fn reply(peer: &mut Peer<Anonymous>, wait: &Wait) -> TestResult<Result<Reply, ClientError>> {
    peer.core.take_reply(pending(wait)?).context("answered")
}

fn refusal(peer: &mut Peer<Anonymous>, wait: &Wait) -> TestResult<ClientErrorEnum> {
    Ok(peer
        .core
        .take_reply(pending(wait)?)
        .context("answered")?
        .err()
        .context("a refusal")?
        .0)
}

fn waiting(pending: Pending, retried: bool, stream: u64) -> (Wait, u64) {
    (Wait::Reply { pending, retried }, stream)
}

/// A commit through session 9, whose next request is 7, of a transaction begun for writing.
fn commit(peer: &mut Peer<Anonymous>) -> TestResult<(Wait, u64)> {
    let session = peer.core.resume_session(9, 7);
    let tx = peer.begin(false)?;
    let p = peer.core.commit(tx, session, peer.now)?;
    let stream = peer.request_stream()?;
    Ok(waiting(p, true, stream))
}

/// Every golden reply, its values from `generate_bodies.py`'s comments and constants.
fn cases() -> Vec<Case> {
    [session_cases(), commit_cases(), drop_and_result_cases()]
        .into_iter()
        .flatten()
        .collect()
}

/// The golden replies of a session, a transaction, a statement and a description.
fn session_cases() -> Vec<Case> {
    vec![
        Case {
            name: "register",
            setup: |peer| {
                let p = peer.core.register(peer.now)?;
                let stream = peer.request_stream()?;
                Ok(waiting(p, false, stream))
            },
            // Register: the session assigned, 0x1122_3344_5566_7788.
            check: |peer, wait| {
                let session = registered(reply(peer, wait)?)?;
                assert_eq!(session.number(), 0x1122_3344_5566_7788);
                Ok(())
            },
        },
        Case {
            name: "begin",
            setup: |peer| {
                let p = peer.core.begin(0, true, peer.now)?;
                let stream = peer.request_stream()?;
                Ok(waiting(p, false, stream))
            },
            // Begin: namespace 3, read version 0xAB_CDEF_0002, read-only.
            check: |peer, wait| {
                let tx = begun(reply(peer, wait)?)?;
                assert_eq!(
                    (tx.namespace(), tx.read_version(), tx.read_only()),
                    (3, 0xAB_CDEF_0002, true)
                );
                Ok(())
            },
        },
        Case {
            name: "prepare_reply",
            setup: |peer| {
                let g = golden("prepare")?;
                let frame = FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE)?;
                let p = peer.core.prepare(&view::<PrepareView>(&frame)?, peer.now)?;
                let stream = peer.request_stream()?;
                Ok(waiting(p, false, stream))
            },
            // Prepare's reply: statement 1, its schema version 0x5C4E_3A2B_1D0F_7E6A, one Term column.
            check: |peer, wait| {
                let statement = prepared(reply(peer, wait)?)?;
                assert_eq!(
                    (statement.id(), statement.schema(), statement.result_types()),
                    (1, 0x5C4E_3A2B_1D0F_7E6A, &[ColumnType::Term][..])
                );
                Ok(())
            },
        },
        Case {
            name: "describe_reply",
            setup: |peer| {
                let p = peer.core.describe(0, peer.now)?;
                let stream = peer.request_stream()?;
                Ok(waiting(p, false, stream))
            },
            // Describe's reply: read version 42, the description the wire's own view reads from the golden bytes.
            check: |peer, wait| {
                let describe = described(reply(peer, wait)?)?;
                let g = golden("describe_reply")?;
                let expected =
                    view::<DescribeView>(&FrameView::validate(&g, CLUSTER, MAX_FRAME_SIZE)?)?;
                assert_eq!((describe.read_version(), &describe), (42, &expected));
                Ok(())
            },
        },
    ]
}

/// The golden replies of a commit: its answer and its refusals.
fn commit_cases() -> Vec<Case> {
    vec![
        Case {
            name: "commit",
            setup: commit,
            // Commit: version 1,000, request 7, results a counter add at -5 and a compare-and-set.
            check: |peer, wait| {
                let committed = committed(reply(peer, wait)?)?;
                assert_eq!((committed.version(), committed.request()), (1_000, 7));
                assert_eq!(
                    committed.results(),
                    &[
                        OperationResult::counter(-5),
                        OperationResult::compare_and_set()
                    ]
                );
                Ok(())
            },
        },
        Case {
            name: "error",
            setup: commit,
            // Error: a conflict on a scanned range, detail 3, retryable.
            check: |peer, wait| {
                let error = refusal(peer, wait)?;
                assert!(
                    matches!(
                        &error,
                        ClientErrorEnum::Refused {
                            code: ErrorCode::Conflict,
                            detail: 3,
                            value: None,
                            ..
                        }
                    ),
                    "{error:?}"
                );
                Ok(())
            },
        },
        Case {
            name: "error_operation",
            setup: commit,
            // Error: a failed compare-and-set, detail 0, the value found the literal "done".
            check: |peer, wait| {
                let error = refusal(peer, wait)?;
                let done = RequestTerm::Literal {
                    lexical: "done".into(),
                    datatype: None,
                };
                assert!(
                    matches!(&error, ClientErrorEnum::Refused { code: ErrorCode::OperationFailed, detail: 0, value: Some(v), .. } if *v == done),
                    "{error:?}"
                );
                Ok(())
            },
        },
    ]
}

/// The golden replies of a drop and a read's result.
fn drop_and_result_cases() -> Vec<Case> {
    vec![
        Case {
            name: "drop_reply",
            setup: |peer| {
                let session = peer.core.resume_session(9, 8);
                let p = peer.core.drop_command(
                    DropCommand::graph(3, "http://ex.org/g"),
                    session,
                    peer.now,
                )?;
                let stream = peer.request_stream()?;
                Ok(waiting(p, true, stream))
            },
            // Drop's reply: session 9's request 8, the drop's commit version 2002.
            check: |peer, wait| {
                let dropped = dropped(reply(peer, wait)?)?;
                assert_eq!((dropped.version(), dropped.request()), (2_002, 8));
                Ok(())
            },
        },
        Case {
            name: "result",
            setup: |peer| {
                let reader = peer.begin(true)?;
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
                let stream = peer.core.read(&reader, &read, peer.now)?;
                let id = peer.request_stream()?;
                assert_eq!(stream.stream(), id);
                Ok((Wait::Batch(stream), id))
            },
            check: result_values,
        },
    ]
}

/// The golden Result read as its values: two columns of three rows, column 0 terms 0, 0, 3, column 1 terms 1, 2 and
/// unbound; resume key "rk"; flagged last.
fn result_values(peer: &mut Peer<Anonymous>, wait: &Wait) -> TestResult {
    let stream = if let Wait::Batch(stream) = wait {
        Some(stream)
    } else {
        None
    }
    .context("a batch")?;
    let polled = peer.core.next_batch(stream, peer.now, |v| {
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
    assert!(matches!(
        peer.core.next_batch(stream, peer.now, |_| ()),
        Ok(BatchPoll::Done)
    ));
    Ok(())
}

/// Every golden reply crosses every fault class under each seed: read as its golden values when it is taken at its
/// place, else the connection given up by the rule and what waited for it failed, retried or finished.
#[test]
fn test_every_golden_reply_under_every_fault_is_read_or_refused_by_the_connections_rule()
-> TestResult {
    for (class, faults) in classes() {
        let mut tally = Tally::default();
        for case in cases() {
            for seed in 1..=SEEDS {
                let mut peer = Peer::welcomed()?;
                let (wait, stream) = (case.setup)(&mut peer)?;
                let frame = restream(&golden(case.name)?, stream);
                peer.cross(faults, seed, &frame, &wait, case.check, &mut tally)?;
            }
        }
        // The evidence of what the networks did and what became of the frames, shown with `--nocapture`.
        println!("replies, {class}: {tally:?}");
        let total = (cases().len() as u64 * SEEDS) as u32;
        assert_eq!(
            tally.delivered + tally.refused + tally.lost,
            total,
            "{class}"
        );
        match class {
            "delay" => {
                assert_eq!(tally.delivered, total, "{class}: every frame arrives");
                assert!(tally.delayed > 0, "{class}: {tally:?}");
            }
            "split" => {
                assert_eq!(tally.delivered, total, "{class}: every frame arrives");
                assert!(tally.split > 0, "{class}: {tally:?}");
            }
            "duplicate" => {
                assert_eq!(
                    tally.delivered, total,
                    "{class}: the copy is refused after it"
                );
                assert!(tally.duplicated > 0, "{class}: {tally:?}");
            }
            _ => assert!(
                tally.delivered > 0 && tally.refused > 0 && tally.lost > 0,
                "{class}: {tally:?}"
            ),
        }
    }
    Ok(())
}

/// The golden Welcome of a server asking for no authentication crosses every fault class: the connection up with its
/// values when taken, else down.
#[test]
fn test_the_golden_welcome_under_every_fault_is_read_or_refused_by_the_connections_rule()
-> TestResult {
    for (class, faults) in classes() {
        let mut tally = Tally::default();
        for seed in 1..=SEEDS * 4 {
            let mut peer = Peer::connecting(Anonymous)?;
            // The golden Welcome: protocol 1, max_frame at its floor of 524,288, 64 streams, initial credit 4, the
            // server "ultragraph 0.1", no authentication.
            let check = |welcomed: &mut Peer<Anonymous>, _: &Wait| -> TestResult {
                assert!(welcomed.core.is_ready());
                let w = welcomed.core.welcome().context("welcomed")?;

                assert_eq!(
                    (
                        w.protocol(),
                        w.max_frame(),
                        w.max_streams(),
                        w.initial_credit(),
                        w.server(),
                        w.authenticates()
                    ),
                    (1, 1 << 19, 64, 4, "ultragraph 0.1", false)
                );
                Ok(())
            };
            peer.cross(
                faults,
                seed,
                &golden("welcome_no_auth")?,
                &Wait::Connection,
                check,
                &mut tally,
            )?;
        }
        println!("welcome, {class}: {tally:?}");
        assert!(tally.delivered > 0, "{class}: {tally:?}");
    }
    Ok(())
}

/// The golden Authenticate exchange, RFC 7677's: each of the server's three frames crosses every fault class in turn,
/// the frames before it taken directly. The client answers each with its golden frame and ends as principal 7 with the
/// golden expiry; a frame not taken at its place gives the connection up, the client not ready.
#[test]
fn test_the_golden_authenticate_exchange_under_every_fault() -> TestResult {
    let steps = [
        "welcome",
        "authenticate_server_first",
        "authenticate_server_final",
    ];
    // The client's answer to each server frame, golden byte for byte: its first message after the Welcome, its final
    // after the server's first (`generate_bodies.py`: the client's frames 2 and 3); after the server's final, principal
    // 7 and the expiry 1,798,761,600,000,000 µs.
    let check = |step: usize| {
        move |peer: &mut Peer<RfcLogin>, _: &Wait| -> TestResult {
            match step {
                0 => assert_eq!(
                    peer.core.transmit(),
                    Some(golden("authenticate_client_first")?)
                ),
                1 => assert_eq!(
                    peer.core.transmit(),
                    Some(golden("authenticate_client_final")?)
                ),
                _ => {
                    assert!(peer.core.is_ready());
                    assert_eq!(peer.core.principal(), Some(7));
                    assert_eq!(peer.core.expires(), Some(1_798_761_600_000_000));
                }
            }
            Ok(())
        }
    };
    for (class, faults) in classes() {
        let mut tally = Tally::default();
        for under in 0..steps.len() {
            for seed in 1..=SEEDS {
                let mut peer = Peer::connecting(rfc_login()?)?;
                for (step, name) in steps.iter().enumerate().take(under) {
                    peer.direct(&golden(name)?)?;
                    check(step)(&mut peer, &Wait::Connection)?;
                }
                let frame = golden(steps[under])?;
                peer.cross(
                    faults,
                    seed,
                    &frame,
                    &Wait::Connection,
                    check(under),
                    &mut tally,
                )?;
            }
        }
        println!("authenticate, {class}: {tally:?}");
        assert!(tally.delivered > 0, "{class}: {tally:?}");
    }
    Ok(())
}

/// The client's Drop is the golden Drop byte for byte, but for the stream it takes: graph <http://ex.org/g> of
/// namespace 3, through session 9's request 8 (`generate_bodies.py`'s Drop).
#[test]
fn test_the_drop_the_client_sends_is_its_golden_frame() -> TestResult {
    let mut peer = Peer::welcomed()?;

    let session = peer.core.resume_session(9, 8);
    let _pending: Pending =
        peer.core
            .drop_command(DropCommand::graph(3, "http://ex.org/g"), session, peer.now)?;
    let frame = peer.core.transmit().context("the Drop")?;
    // The client's second frame, after the Hello, on its first stream.
    assert_eq!(frame, numbered(&restream(&golden("drop")?, 1), 2));
    Ok(())
}
