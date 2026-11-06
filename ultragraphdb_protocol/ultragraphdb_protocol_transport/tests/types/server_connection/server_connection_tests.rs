/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::{
    AUTHENTICATION_TIMEOUT, Endpoint, IDLE_LIMIT, Inbound, ServerConnection, TransportError,
    TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    Abort, Authenticate, AuthenticateView, Begin, BodyBuilder, BodyView, Cancel, Command, Commit,
    CommitOperation, ComputeRead, ConditionalWrite, Credit, Describe, DirectWrite, DropCommand,
    Entries, EntriesEnd, ErrorCode, ErrorReply, ErrorReplyView, Execute, FLAG_LAST, FrameView,
    Header, HeaderView, Held, Hello, Install, Language, MAX_FRAME_FLOOR, MAX_FRAME_SIZE, Operation,
    PatternSlot, Prepare, QuadPattern, Query, Read, ReadLevel, Register, ResultBatch, Scan, ScanAt,
    ScanRange, TermTable, Welcome, WelcomeView, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::mechanism::{
    EXPIRES, GIANT_CHALLENGE, GIANT_WELCOME, TEST, TestAuthenticator, User,
};
use crate::utils::peer::{CLUSTER, Peer, start};

const ALICE: User = User {
    name: "alice",
    password: "pw",
    principal: 7,
    valid_for: None,
};

const BOB: User = User {
    name: "bob",
    password: "pw",
    principal: 8,
    valid_for: None,
};

/// A user the server's mechanism welcomes with a final message past the largest frame: alice's principal, so it
/// renews her connection.
const GIANT: User = User {
    name: GIANT_WELCOME,
    password: "pw",
    principal: 7,
    valid_for: Some(Duration::from_secs(60)),
};

/// A user whose credential lasts a minute.
const CAROL: User = User {
    name: "carol",
    password: "pw",
    principal: 9,
    valid_for: Some(Duration::from_secs(60)),
};

fn server(mechanisms: &[&str]) -> Welcome {
    Welcome::new(1, 0, "server")
        .with_max_streams(4)
        .with_initial_credit(2)
        .with_mechanisms(mechanisms.iter().copied())
}

type Connection = ServerConnection<TestAuthenticator>;

fn connection(mechanisms: &[&str], now: Instant) -> Connection {
    ServerConnection::new(
        CLUSTER,
        server(mechanisms),
        TestAuthenticator::new(vec![ALICE, BOB, CAROL, GIANT]),
        now,
    )
}

/// Frames a connection sent, as (stream, command, flags, bytes).
type Frames = Vec<(u64, u16, u8, Vec<u8>)>;

/// The frames the connection sends.
fn sent(connection: &mut Connection) -> TestResult<Frames> {
    std::iter::from_fn(|| connection.next_frame())
        .map(|(stream, frame)| -> TestResult<_> {
            let header = HeaderView::new(&frame)?;
            Ok((stream, header.command(), header.flags(), frame))
        })
        .collect()
}

/// Takes the frames the connection sends, each checked to hold a header, where a test has no use for them.
fn drain(connection: &mut Connection) -> TestResult {
    let _frames: Frames = sent(connection)?;
    Ok(())
}

/// The code and detail of an Error frame.
fn error(frame: &[u8]) -> TestResult<(ErrorCode, u64)> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    let error = ErrorReplyView::from_frame(&view)?;
    Ok((error.code(), error.detail()))
}

fn hello() -> Hello {
    Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "client")
}

/// Sends `body` on `stream`, a request the connection takes as one, at `now`.
fn request<B: BodyBuilder>(
    connection: &mut Connection,
    peer: &mut Peer,
    stream: u64,
    body: &B,
    now: Instant,
) -> TestResult {
    assert_eq!(
        connection.receive(&peer.send(stream, body)?, now)?,
        Inbound::Request {
            stream,
            command: B::COMMAND
        }
    );
    Ok(())
}

/// A connection past its Hello.
fn welcomed(mechanisms: &[&str], peer: &mut Peer, now: Instant) -> TestResult<Connection> {
    let mut connection = connection(mechanisms, now);
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    drain(&mut connection)?;
    Ok(connection)
}

/// `name`'s exchange with `password` on `connection`; what its final step made of it.
fn exchange(
    connection: &mut Connection,
    peer: &mut Peer,
    name: &str,
    password: &str,
    now: Instant,
) -> TestResult<Inbound> {
    let first = Authenticate::new(0, TEST, format!("user={name}").into_bytes());
    assert_eq!(
        connection.receive(&peer.send(0, &first)?, now)?,
        Inbound::Handled
    );
    // The challenge on stream 0; a read may stream beside it.
    let frames = sent(connection)?;
    let challenge: Vec<_> = frames.iter().filter(|f| f.0 == 0).collect();
    assert_eq!(challenge.len(), 1);
    let view = FrameView::validate(&challenge[0].3, CLUSTER, MAX_FRAME_SIZE)?;
    let message = AuthenticateView::from_frame(&view)?;
    assert_eq!((message.step(), message.message()), (1, &b"challenge"[..]));
    let proof = Authenticate::new(2, "", format!("pass={password}").into_bytes());
    Ok(connection.receive(&peer.send(0, &proof)?, now)?)
}

/// A connection authenticated as alice.
fn serving(peer: &mut Peer, now: Instant) -> TestResult<Connection> {
    let mut connection = welcomed(&[TEST], peer, now)?;
    assert_eq!(
        exchange(&mut connection, peer, "alice", "pw", now)?,
        Inbound::Authenticated { principal: 7 }
    );
    drain(&mut connection)?;
    Ok(connection)
}

fn read(transaction: u64) -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    Read::new(transaction, 0, terms, pattern, Vec::new())
}

fn result(stream: u64, last: bool) -> TestResult<Vec<u8>> {
    Ok(ResultBatch::new(1).frame(
        Header::new(CLUSTER, Command::Result)
            .with_stream(stream)
            .with_flags(if last { FLAG_LAST } else { 0 }),
    )?)
}

#[test]
fn test_a_hello_is_welcomed_with_the_servers_mechanisms_at_sequence_one() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[TEST], now);
    assert!(connection.welcome().is_none());
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    let frames = sent(&mut connection)?;
    assert_eq!(frames.len(), 1);
    let (stream, command, _, bytes) = &frames[0];
    assert_eq!((*stream, *command), (0, Command::Welcome.number()));
    assert_eq!(HeaderView::new(bytes)?.sequence(), 1);
    let view = FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let welcome = WelcomeView::from_frame(&view)?;
    assert!(welcome.authenticates());
    assert_eq!(welcome.mechanisms().iter().collect::<Vec<_>>(), [TEST]);
    assert_eq!(connection.welcome().map(Welcome::protocol), Some(1));
    assert_eq!(connection.principal(), None, "not yet authenticated");
    Ok(())
}

#[test]
fn test_a_first_frame_other_than_hello_closes_with_a_protocol_error() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[TEST], now);
    assert_eq!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false))?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let frames = sent(&mut connection)?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0, 0, "on stream 0");
    assert_eq!(error(&frames[0].3)?.0, ErrorCode::Protocol);
    assert!(connection.is_closing());
    Ok(())
}

#[test]
fn test_an_exchange_authenticates_the_principal_its_final_message_names() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    assert_eq!(
        exchange(&mut connection, &mut peer, "alice", "pw", now)?,
        Inbound::Authenticated { principal: 7 }
    );
    let frames = sent(&mut connection)?;
    let view = FrameView::validate(&frames[0].3, CLUSTER, MAX_FRAME_SIZE)?;
    let last = AuthenticateView::from_frame(&view)?;
    assert!(last.is_final());
    assert_eq!((last.step(), last.principal(), last.expires()), (3, 7, 0));
    assert_eq!(last.message(), b"welcome");
    assert_eq!(connection.principal(), Some(7));
    Ok(())
}

#[test]
fn test_a_failed_exchange_closes_with_code_19_saying_nothing_of_why() -> TestResult {
    let now = start();
    // A wrong password and a name no principal holds give the same frame.
    let mut frames = Vec::new();
    for (name, password) in [("alice", "wrong"), ("nobody", "pw")] {
        let mut peer = Peer::default();
        let mut connection = welcomed(&[TEST], &mut peer, now)?;
        assert_eq!(
            exchange(&mut connection, &mut peer, name, password, now)?,
            Inbound::Closing {
                code: Some(ErrorCode::AuthenticationFailed)
            }
        );
        let out = sent(&mut connection)?;
        assert_eq!(out.len(), 1);
        assert_eq!(error(&out[0].3)?, (ErrorCode::AuthenticationFailed, 0));
        assert!(connection.is_closing());
        frames.push(out[0].3.clone());
    }
    assert_eq!(frames[0], frames[1], "the two refusals are the same bytes");
    // A mechanism the server does not offer.
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    let other = Authenticate::new(0, "PLAIN", b"x".to_vec());
    assert_eq!(
        connection.receive(&peer.send(0, &other)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::AuthenticationFailed)
        }
    );
    Ok(())
}

#[test]
fn test_a_request_before_authentication_is_refused_on_its_stream() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    assert_eq!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false))?, now)?,
        Inbound::Refused {
            stream: 1,
            code: ErrorCode::Unauthenticated
        }
    );
    let out = sent(&mut connection)?;
    assert_eq!(
        (out[0].0, error(&out[0].3)?.0),
        (1, ErrorCode::Unauthenticated)
    );
    assert!(!connection.is_closing(), "it stays open for authentication");
    Ok(())
}

#[test]
fn test_a_frame_on_a_stream_that_ended_is_dropped() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[], &mut peer, now)?;
    assert_eq!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false))?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Begin
        }
    );
    connection.refuse(1, &ErrorReply::new(ErrorCode::Refused, "ended"))?;
    let _out = sent(&mut connection)?;
    let late = Commit::new([0; 16], 0, 0);
    assert_eq!(
        connection.receive(&peer.send(1, &late)?, now)?,
        Inbound::Handled
    );
    assert!(!connection.is_closing(), "a late frame closes nothing");
    assert_eq!(sent(&mut connection)?.len(), 0, "nothing answers it");
    Ok(())
}

/// The endpoint's rule (`PROTOCOL.md` §2.5 step 3) comes before the late rule: a role command, or a frame no client
/// sends, on a stream that ended closes the connection as it does on a fresh number.
#[test]
fn test_a_frame_the_endpoint_refuses_on_a_stream_that_ended_closes_the_connection() -> TestResult {
    let now = start();
    for command in [Command::Scan, Command::Result] {
        let mut peer = Peer::default();
        let mut connection = serving(&mut peer, now)?;
        request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
        connection.refuse(1, &ErrorReply::new(ErrorCode::Refused, "ended"))?;
        drain(&mut connection)?;
        assert_eq!(
            connection.receive(&peer.command(1, command)?, now)?,
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "{command:?}"
        );
        assert!(connection.is_closing(), "{command:?}");
    }
    Ok(())
}

/// A Credit or a Cancel opens no stream (`PROTOCOL.md` §11.2), so a request on a lower number that was never opened is
/// still a request.
#[test]
fn test_a_credit_or_cancel_on_a_stream_never_opened_leaves_lower_numbers_new() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    assert_eq!(
        connection.receive(&peer.send(7, &Credit::new(1))?, now)?,
        Inbound::Handled
    );
    assert_eq!(
        connection.receive(&peer.send(9, &Cancel)?, now)?,
        Inbound::Handled
    );
    request(&mut connection, &mut peer, 4, &Begin::new(0, 0, false), now)?;
    Ok(())
}

#[test]
fn test_a_connection_past_the_bound_is_refused_in_place_of_its_welcome() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[], now).past_connection_bound(2);
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::StreamLimit)
        }
    );
    let out = sent(&mut connection)?;
    let (code, detail) = error(&out[0].3)?;
    assert_eq!((out[0].0, code, detail), (0, ErrorCode::StreamLimit, 2));
    assert!(connection.is_closing());
    Ok(())
}

#[test]
fn test_an_unfinished_authentication_times_out_with_code_20() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    assert_eq!(
        connection.next_deadline(),
        Some(now + AUTHENTICATION_TIMEOUT)
    );
    assert_eq!(
        connection.handle_timeout(now + AUTHENTICATION_TIMEOUT - Duration::from_millis(1))?,
        None
    );
    assert_eq!(
        connection.handle_timeout(now + AUTHENTICATION_TIMEOUT)?,
        Some(Inbound::Closing {
            code: Some(ErrorCode::Unauthenticated)
        })
    );
    let out = sent(&mut connection)?;
    assert_eq!(error(&out[0].3)?.0, ErrorCode::Unauthenticated);
    assert_eq!(
        connection.next_deadline(),
        None,
        "a closing connection has no deadline"
    );
    // An authenticated connection is past the timeout: its next deadline is the idle limit.
    let mut authenticated_peer = Peer::default();
    let authenticated = serving(&mut authenticated_peer, now)?;
    assert_eq!(authenticated.next_deadline(), Some(now + IDLE_LIMIT));
    Ok(())
}

#[test]
fn test_a_server_without_authentication_serves_at_once() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[], now);
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    let out = sent(&mut connection)?;
    let view = FrameView::validate(&out[0].3, CLUSTER, MAX_FRAME_SIZE)?;
    assert!(!WelcomeView::from_frame(&view)?.authenticates());
    assert_eq!(connection.principal(), Some(0), "the anonymous principal");
    assert_eq!(
        connection.receive(&peer.send(1, &Register::new(0u128.to_be_bytes()))?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Register
        }
    );
    // An exchange where none is offered is a protocol error.
    let first = Authenticate::new(0, TEST, b"user=alice".to_vec());
    assert_eq!(
        connection.receive(&peer.send(0, &first)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

#[test]
fn test_a_credential_is_renewed_on_the_open_connection_while_a_read_streams() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    assert_eq!(
        exchange(&mut connection, &mut peer, "carol", "pw", now)?,
        Inbound::Authenticated { principal: 9 }
    );
    let out = sent(&mut connection)?;
    let view = FrameView::validate(&out[0].3, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(AuthenticateView::from_frame(&view)?.expires(), EXPIRES);
    assert_eq!(
        connection.next_deadline(),
        Some(now + Duration::from_secs(60))
    );
    // A transaction and a read streaming.
    assert!(matches!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false))?, now)?,
        Inbound::Request { .. }
    ));
    assert_eq!(
        connection.receive(&peer.send(3, &read(1))?, now)?,
        Inbound::Request {
            stream: 3,
            command: Command::Read
        }
    );
    connection.outbound().push(3, result(3, false)?)?;
    // Renewed 50 seconds in, the deadline moves to a minute from the renewal.
    let later = now + Duration::from_secs(50);
    assert_eq!(
        exchange(&mut connection, &mut peer, "carol", "pw", later)?,
        Inbound::Authenticated { principal: 9 }
    );
    assert_eq!(
        connection.next_deadline(),
        Some(later + Duration::from_secs(60))
    );
    assert_eq!(
        connection.handle_timeout(now + Duration::from_secs(61))?,
        None
    );
    // The read is open past the first expiry and goes on streaming.
    connection.outbound().push(3, result(3, true)?)?;
    let streams: Vec<u64> = sent(&mut connection)?.iter().map(|f| f.0).collect();
    assert_eq!(
        streams,
        vec![0, 3],
        "the final Authenticate, then the read, going on after the renewal"
    );
    Ok(())
}

#[test]
fn test_renewing_as_another_principal_is_refused_with_code_19() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    assert_eq!(
        exchange(&mut connection, &mut peer, "bob", "pw", now)?,
        Inbound::Closing {
            code: Some(ErrorCode::AuthenticationFailed)
        }
    );
    Ok(())
}

#[test]
fn test_an_expired_credential_closes_with_code_22() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    assert_eq!(
        exchange(&mut connection, &mut peer, "carol", "pw", now)?,
        Inbound::Authenticated { principal: 9 }
    );
    drain(&mut connection)?;
    assert_eq!(
        connection.handle_timeout(now + Duration::from_secs(60))?,
        Some(Inbound::Closing {
            code: Some(ErrorCode::CredentialExpired)
        })
    );
    let out = sent(&mut connection)?;
    assert_eq!(
        (out[0].0, error(&out[0].3)?.0),
        (0, ErrorCode::CredentialExpired)
    );
    Ok(())
}

#[test]
fn test_an_idle_connection_closes_without_a_frame() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    // A frame resets the idle limit.
    let later = now + Duration::from_secs(300);
    request(
        &mut connection,
        &mut peer,
        1,
        &Register::new(0u128.to_be_bytes()),
        later,
    )?;
    assert_eq!(connection.handle_timeout(now + IDLE_LIMIT)?, None);
    assert_eq!(
        connection.handle_timeout(later + IDLE_LIMIT)?,
        Some(Inbound::Closing { code: None })
    );
    assert!(sent(&mut connection)?.is_empty());
    assert!(connection.is_closing());
    Ok(())
}

#[test]
fn test_a_request_on_stream_zero_or_on_an_open_stream_closes_the_connection() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    assert_eq!(
        connection.receive(&peer.send(0, &Begin::new(0, 0, false))?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let out = sent(&mut connection)?;
    assert_eq!((out[0].0, error(&out[0].3)?.0), (0, ErrorCode::Protocol));
    let mut reopening_peer = Peer::default();
    let mut reopening = serving(&mut reopening_peer, now)?;
    request(
        &mut reopening,
        &mut reopening_peer,
        1,
        &Begin::new(0, 0, false),
        now,
    )?;
    assert_eq!(
        reopening.receive(
            &reopening_peer.send(1, &Register::new(0u128.to_be_bytes()))?,
            now
        )?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

#[test]
fn test_a_stream_past_the_limit_is_refused_on_it_and_the_connection_stays() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    for stream in 1..=4 {
        request(
            &mut connection,
            &mut peer,
            stream,
            &Begin::new(0, 0, false),
            now,
        )?;
    }
    assert_eq!(
        connection.receive(&peer.send(5, &Register::new(0u128.to_be_bytes()))?, now)?,
        Inbound::Refused {
            stream: 5,
            code: ErrorCode::StreamLimit
        }
    );
    let out = sent(&mut connection)?;
    let refusal = out
        .iter()
        .find(|f| f.0 == 5)
        .context("the refusal on stream 5")?;
    assert_eq!(error(&refusal.3)?, (ErrorCode::StreamLimit, 4));
    assert_eq!(refusal.2, FLAG_LAST);
    assert!(!connection.is_closing());
    Ok(())
}

#[test]
fn test_a_read_runs_on_its_own_stream_naming_an_open_transaction() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    assert_eq!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false))?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Begin
        }
    );
    assert!(connection.outbound().is_transaction(1));
    assert_eq!(
        connection.receive(&peer.send(2, &read(1))?, now)?,
        Inbound::Request {
            stream: 2,
            command: Command::Read
        }
    );
    assert_eq!(
        connection.outbound().credit(2),
        Some(2),
        "the Welcome's initial credit"
    );
    // A read naming no open transaction is refused on its stream.
    assert_eq!(
        connection.receive(&peer.send(3, &read(9))?, now)?,
        Inbound::Refused {
            stream: 3,
            code: ErrorCode::Protocol
        }
    );
    assert!(!connection.is_closing());
    Ok(())
}

#[test]
fn test_cancels_end_a_read_alone_or_a_transaction_with_its_reads() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    request(&mut connection, &mut peer, 2, &read(1), now)?;
    request(&mut connection, &mut peer, 3, &read(1), now)?;
    assert_eq!(
        connection.receive(&peer.send(2, &Cancel)?, now)?,
        Inbound::Cancelled {
            stream: 2,
            transaction: false
        }
    );
    assert!(connection.outbound().is_transaction(1));
    assert_eq!(
        connection.receive(&peer.send(1, &Cancel)?, now)?,
        Inbound::Cancelled {
            stream: 1,
            transaction: true
        }
    );
    assert!(
        !connection.outbound().is_open(3),
        "the transaction's read ended with it"
    );
    Ok(())
}

#[test]
fn test_credit_is_granted_and_writes_go_to_their_transaction() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    request(&mut connection, &mut peer, 2, &read(1), now)?;
    assert_eq!(
        connection.receive(&peer.send(2, &Credit::new(5))?, now)?,
        Inbound::Handled
    );
    assert_eq!(connection.outbound().credit(2), Some(7));
    let write = Write::new(TermTable::new(), Vec::new());
    assert_eq!(
        connection.receive(&peer.send(1, &write)?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Write
        }
    );
    // A Write on a stream holding no transaction is refused on it.
    assert_eq!(
        connection.receive(&peer.send(4, &write)?, now)?,
        Inbound::Refused {
            stream: 4,
            code: ErrorCode::Protocol
        }
    );
    // An Abort ends its transaction and is the session's to apply.
    assert_eq!(
        connection.receive(&peer.send(1, &Abort)?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Abort
        }
    );
    assert!(!connection.outbound().is_open(1));
    assert!(!connection.outbound().is_open(2));
    Ok(())
}

#[test]
fn test_a_commit_retried_on_a_new_stream_is_a_request_of_its_own() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    assert_eq!(
        connection.receive(&peer.send(6, &Commit::new(3u128.to_be_bytes(), 2, 0))?, now)?,
        Inbound::Request {
            stream: 6,
            command: Command::Commit
        }
    );
    assert!(connection.outbound().is_open(6));
    assert!(!connection.outbound().is_transaction(6));
    Ok(())
}

#[test]
fn test_the_sessions_refusal_ends_its_stream_and_a_transactions_reads() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    request(&mut connection, &mut peer, 2, &read(1), now)?;
    connection.refuse(1, &ErrorReply::new(ErrorCode::Protocol, "a repeated term"))?;
    let out = sent(&mut connection)?;
    let refusal = out
        .iter()
        .find(|f| f.0 == 1)
        .context("the refusal on stream 1")?;
    assert_eq!(refusal.2, FLAG_LAST);
    assert_eq!(error(&refusal.3)?.0, ErrorCode::Protocol);
    assert!(!connection.outbound().is_open(1));
    assert!(!connection.outbound().is_open(2));
    assert!(!connection.is_closing());
    Ok(())
}

#[test]
fn test_a_frame_out_of_sequence_closes_the_connection() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    peer.skip();
    assert_eq!(
        connection.receive(&peer.send(1, &Register::new(0u128.to_be_bytes()))?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let out = sent(&mut connection)?;
    assert_eq!((out[0].0, error(&out[0].3)?.0), (0, ErrorCode::Protocol));
    // Nothing more is taken once closing.
    assert_eq!(
        connection.receive(&peer.send(2, &Register::new(0u128.to_be_bytes()))?, now)?,
        Inbound::Closing { code: None }
    );
    assert!(sent(&mut connection)?.is_empty());
    Ok(())
}

#[test]
fn test_every_frame_the_connection_sends_is_numbered_in_order() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    let first = Authenticate::new(0, TEST, b"user=alice".to_vec());
    assert_eq!(
        connection.receive(&peer.send(0, &first)?, now)?,
        Inbound::Handled
    );
    let proof = Authenticate::new(2, "", b"pass=pw".to_vec());
    assert_eq!(
        connection.receive(&peer.send(0, &proof)?, now)?,
        Inbound::Authenticated { principal: 7 }
    );
    // The Welcome was 1; the challenge and the final message follow it.
    let sequences = sent(&mut connection)?
        .iter()
        .map(|f| Ok(HeaderView::new(&f.3)?.sequence()))
        .collect::<TestResult<Vec<u128>>>()?;
    assert_eq!(sequences, vec![2, 3]);
    Ok(())
}

#[test]
fn test_a_new_request_on_an_open_stream_closes_before_its_body_is_read() -> TestResult {
    // A Read whose body breaks a rule, on a stream already open: the stream rule closes the connection first.
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    request(
        &mut connection,
        &mut peer,
        3,
        &Register::new(0u128.to_be_bytes()),
        now,
    )?;
    let mut bad = read(1).frame(Header::new(CLUSTER, Command::Read).with_stream(3))?;
    // flags bit 1: a Read's body rule, refused on its stream were the stream new.
    bad[128 + 12] = 2;
    peer.skip();
    Header::stamp_sequence(&mut bad, 6)?;
    assert_eq!(
        connection.receive(&peer.raw(&bad)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

#[test]
fn test_every_failed_exchange_is_refused_with_the_same_bytes() -> TestResult {
    // A wrong password, and a message the mechanism does not take at its step: two failures, one refusal.
    let now = start();
    let refusal = |message: &[u8]| -> TestResult<Vec<u8>> {
        let mut peer = Peer::default();
        let mut connection = welcomed(&[TEST], &mut peer, now)?;
        let first = Authenticate::new(0, TEST, b"user=alice".to_vec());
        assert_eq!(
            connection.receive(&peer.send(0, &first)?, now)?,
            Inbound::Handled
        );
        drain(&mut connection)?;
        let second = Authenticate::new(2, "", message.to_vec());
        assert_eq!(
            connection.receive(&peer.send(0, &second)?, now)?,
            Inbound::Closing {
                code: Some(ErrorCode::AuthenticationFailed)
            }
        );
        Ok(sent(&mut connection)?
            .into_iter()
            .next()
            .context("the refusal")?
            .3)
    };
    assert_eq!(refusal(b"pass=wrong")?, refusal(b"garbage")?);
    Ok(())
}

#[test]
fn test_every_frame_on_stream_zero_but_hello_and_authenticate_closes_the_connection() -> TestResult
{
    let now = start();
    let write = Write::new(TermTable::new(), Vec::new());
    for frame in [0, 1, 2] {
        let mut peer = Peer::default();
        let mut connection = serving(&mut peer, now)?;
        let received = match frame {
            0 => peer.send(0, &Credit::new(1))?,
            1 => peer.send(0, &Cancel)?,
            _ => peer.send(0, &write)?,
        };
        assert_eq!(
            connection.receive(&received, now)?,
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "frame {frame}"
        );
    }
    Ok(())
}

#[test]
fn test_a_closing_connection_sends_nothing_after_its_error() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    connection.outbound().push(
        1,
        Commit::new(1u128.to_be_bytes(), 1, 2).frame(
            Header::new(CLUSTER, Command::Commit)
                .with_stream(1)
                .with_flags(FLAG_LAST),
        )?,
    )?;
    assert_eq!(
        connection.receive(&peer.send(0, &Begin::new(0, 0, false))?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let out = sent(&mut connection)?;
    assert_eq!(out.len(), 1, "the Begin's reply and the Commit stay unsent");
    assert_eq!((out[0].0, error(&out[0].3)?.0), (0, ErrorCode::Protocol));
    assert_eq!(connection.next_frame(), None);
    Ok(())
}

/// The connection hands on the largest frame the client's Hello states, for what is sent to that client.
#[test]
fn test_a_welcome_carries_the_clients_max_frame() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[], now);
    let max_frame = MAX_FRAME_FLOOR as u32;
    assert_eq!(
        connection.receive(
            &peer.send(0, &Hello::new(1, 1, 0, max_frame, "client"))?,
            now
        )?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame
        }
    );
    Ok(())
}

/// A Hello whose lowest protocol is above the server's highest is answered with code 16 on stream 0, `detail` the
/// server's highest, and the connection closes: nothing more is read or sent (point 7).
#[test]
fn test_a_hello_above_the_servers_protocols_closes_with_code_16() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[], now);
    assert_eq!(
        connection.receive(
            &peer.send(0, &Hello::new(2, 3, 0, MAX_FRAME_SIZE as u32, "client"))?,
            now
        )?,
        Inbound::Closing {
            code: Some(ErrorCode::ProtocolVersion)
        }
    );
    assert!(connection.is_closing());
    let out = sent(&mut connection)?;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, 0);
    assert_eq!(error(&out[0].3)?, (ErrorCode::ProtocolVersion, 1));
    assert_eq!(
        connection.receive(&peer.send(1, &Register::new(0u128.to_be_bytes()))?, now)?,
        Inbound::Closing { code: None },
        "a closing connection reads nothing"
    );
    assert_eq!(connection.next_frame(), None);
    Ok(())
}

#[test]
fn test_a_cancel_of_a_stream_not_open_is_handled_and_answered_with_nothing() -> TestResult {
    // A Cancel of a stream the connection holds no longer, or never held, changes nothing (PROTOCOL.md section 11.4).
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    assert_eq!(
        connection.receive(&peer.send(5, &Cancel)?, now)?,
        Inbound::Handled
    );
    assert!(sent(&mut connection)?.is_empty());
    assert!(!connection.is_closing());
    Ok(())
}

#[test]
fn test_a_command_only_a_server_sends_closes_the_connection() -> TestResult {
    let now = start();
    for frame in [0, 1, 2] {
        let mut peer = Peer::default();
        let mut connection = serving(&mut peer, now)?;
        let received = match frame {
            0 => peer.send(1, &ResultBatch::new(1))?,
            1 => peer.send(1, &server(&[]))?,
            _ => peer.send(1, &ErrorReply::new(ErrorCode::Refused, "no"))?,
        };
        assert_eq!(
            connection.receive(&received, now)?,
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "frame {frame}"
        );
    }
    Ok(())
}

/// A nanosecond, the finest step an `Instant` takes: a deadline is checked one tick before it.
const TICK: Duration = Duration::from_nanos(1);

#[test]
fn test_limits_set_the_authentication_timeout_and_the_idle_limit() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let authentication = Duration::from_secs(10);
    let idle = Duration::from_secs(20);
    let mut connection = connection(&[TEST], now).with_limits(authentication, idle);
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    assert_eq!(connection.next_deadline(), Some(now + authentication));
    assert_eq!(
        connection.handle_timeout(now + authentication - TICK)?,
        None
    );
    assert_eq!(
        connection.handle_timeout(now + authentication)?,
        Some(Inbound::Closing {
            code: Some(ErrorCode::Unauthenticated)
        })
    );
    // Authenticated, its deadline is the idle limit given, at its boundary exactly.
    let mut authenticated_peer = Peer::default();
    let mut authenticated = self::connection(&[TEST], now).with_limits(authentication, idle);
    assert_eq!(
        authenticated.receive(&authenticated_peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    drain(&mut authenticated)?;
    assert_eq!(
        exchange(
            &mut authenticated,
            &mut authenticated_peer,
            "alice",
            "pw",
            now
        )?,
        Inbound::Authenticated { principal: 7 }
    );
    assert_eq!(authenticated.next_deadline(), Some(now + idle));
    assert_eq!(authenticated.handle_timeout(now + idle - TICK)?, None);
    assert_eq!(
        authenticated.handle_timeout(now + idle)?,
        Some(Inbound::Closing { code: None })
    );
    assert_eq!(
        authenticated.handle_timeout(now + idle + idle)?,
        None,
        "a closing connection has no deadline left"
    );
    Ok(())
}

#[test]
fn test_the_deadline_is_the_earlier_of_the_idle_limit_and_the_expiry() -> TestResult {
    // Carol's credential lasts 60 s; the idle limit is 30 s.
    let now = start();
    let mut peer = Peer::default();
    let mut connection =
        connection(&[TEST], now).with_limits(AUTHENTICATION_TIMEOUT, Duration::from_secs(30));
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    drain(&mut connection)?;
    assert_eq!(
        exchange(&mut connection, &mut peer, "carol", "pw", now)?,
        Inbound::Authenticated { principal: 9 }
    );
    assert_eq!(
        connection.next_deadline(),
        Some(now + Duration::from_secs(30)),
        "idle first"
    );
    // A frame 20 s in moves the idle limit to 50 s; one 40 s in to 70 s, past the expiry at 60 s.
    request(
        &mut connection,
        &mut peer,
        1,
        &Register::new(0u128.to_be_bytes()),
        now + Duration::from_secs(20),
    )?;
    assert_eq!(
        connection.next_deadline(),
        Some(now + Duration::from_secs(50))
    );
    request(
        &mut connection,
        &mut peer,
        3,
        &Register::new(0u128.to_be_bytes()),
        now + Duration::from_secs(40),
    )?;
    assert_eq!(
        connection.next_deadline(),
        Some(now + Duration::from_secs(60)),
        "the expiry first"
    );
    assert_eq!(
        connection.handle_timeout(now + Duration::from_secs(60) - TICK)?,
        None
    );
    assert_eq!(
        connection.handle_timeout(now + Duration::from_secs(60))?,
        Some(Inbound::Closing {
            code: Some(ErrorCode::CredentialExpired)
        })
    );
    Ok(())
}

#[test]
fn test_the_idle_limit_is_reached_at_its_boundary_exactly() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    assert_eq!(connection.handle_timeout(now + IDLE_LIMIT - TICK)?, None);
    assert!(!connection.is_closing());
    assert_eq!(
        connection.handle_timeout(now + IDLE_LIMIT)?,
        Some(Inbound::Closing { code: None })
    );
    Ok(())
}

#[test]
fn test_the_authentication_timeout_runs_from_the_welcome() -> TestResult {
    // PROTOCOL.md section 11.9: the authentication timeout runs from the Welcome to the final Authenticate. A Hello
    // 30 s after the connection was accepted leaves the exchange its full 60 s.
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[TEST], now);
    let hello_at = now + Duration::from_secs(30);
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, hello_at)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    assert_eq!(
        connection.next_deadline(),
        Some(hello_at + AUTHENTICATION_TIMEOUT)
    );
    assert_eq!(
        connection.handle_timeout(now + AUTHENTICATION_TIMEOUT)?,
        None
    );
    assert_eq!(
        connection.handle_timeout(hello_at + AUTHENTICATION_TIMEOUT - TICK)?,
        None
    );
    assert_eq!(
        connection.handle_timeout(hello_at + AUTHENTICATION_TIMEOUT)?,
        Some(Inbound::Closing {
            code: Some(ErrorCode::Unauthenticated)
        })
    );
    // A connection that never says Hello is held to the same time from its acceptance.
    let mut silent = self::connection(&[TEST], now);
    assert_eq!(silent.next_deadline(), Some(now + AUTHENTICATION_TIMEOUT));
    assert_eq!(
        silent.handle_timeout(now + AUTHENTICATION_TIMEOUT)?,
        Some(Inbound::Closing {
            code: Some(ErrorCode::Unauthenticated)
        })
    );
    Ok(())
}

#[test]
fn test_an_authenticate_out_of_step_closes_with_a_protocol_error() -> TestResult {
    let now = start();
    // Step 2 first, where 0 is due.
    let mut early_peer = Peer::default();
    let mut early_connection = welcomed(&[TEST], &mut early_peer, now)?;
    let early = Authenticate::new(2, "", b"pass=pw".to_vec());
    assert_eq!(
        early_connection.receive(&early_peer.send(0, &early)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let out = sent(&mut early_connection)?;
    assert_eq!((out[0].0, error(&out[0].3)?), (0, (ErrorCode::Protocol, 0)));
    // Step 0 again, where 2 is due.
    let mut repeating_peer = Peer::default();
    let mut repeating = welcomed(&[TEST], &mut repeating_peer, now)?;
    let first = Authenticate::new(0, TEST, b"user=alice".to_vec());
    assert_eq!(
        repeating.receive(&repeating_peer.send(0, &first)?, now)?,
        Inbound::Handled
    );
    drain(&mut repeating)?;
    assert_eq!(
        repeating.receive(&repeating_peer.send(0, &first)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    assert_eq!(repeating.principal(), None);
    // Step 0 naming no mechanism breaks the body's rule (PROTOCOL.md section 5.3).
    let mut unnamed_peer = Peer::default();
    let mut unnamed_connection = welcomed(&[TEST], &mut unnamed_peer, now)?;
    let unnamed = Authenticate::new(0, "", b"user=alice".to_vec());
    assert_eq!(
        unnamed_connection.receive(&unnamed_peer.send(0, &unnamed)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

#[test]
fn test_a_credit_of_no_frames_is_refused_on_its_stream() -> TestResult {
    // PROTOCOL.md section 9.2: `frames` at least 1; a body's rule refuses the request on its stream (section 2.6).
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    request(&mut connection, &mut peer, 2, &read(1), now)?;
    assert_eq!(
        connection.receive(&peer.send(2, &Credit::new(0))?, now)?,
        Inbound::Refused {
            stream: 2,
            code: ErrorCode::Protocol
        }
    );
    assert_eq!(connection.outbound().credit(2), Some(2), "nothing granted");
    let out = sent(&mut connection)?;
    let refusal = out
        .iter()
        .find(|f| f.0 == 2)
        .context("the refusal on stream 2")?;
    assert_eq!((refusal.1, refusal.2), (Command::Error.number(), FLAG_LAST));
    assert_eq!(error(&refusal.3)?, (ErrorCode::Protocol, 0));
    assert!(
        !connection.outbound().is_open(2),
        "the refusal ended the read"
    );
    assert!(connection.outbound().is_transaction(1));
    assert!(!connection.is_closing());
    Ok(())
}

#[test]
fn test_a_read_breaking_its_bodys_rule_on_a_new_stream_is_refused_on_it() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    // The Hello, the two Authenticate messages and the Begin take sequences 1 to 4.
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    let mut bad = read(1).frame(Header::new(CLUSTER, Command::Read).with_stream(3))?;
    // flags bit 1 of a Read's command header, which only bit 0 may set.
    bad[128 + 12] = 2;
    peer.skip();
    Header::stamp_sequence(&mut bad, 5)?;
    assert_eq!(
        connection.receive(&peer.raw(&bad)?, now)?,
        Inbound::Refused {
            stream: 3,
            code: ErrorCode::Protocol
        }
    );
    assert!(!connection.outbound().is_open(3));
    assert!(!connection.is_closing());
    Ok(())
}

#[test]
fn test_an_execute_runs_on_its_own_stream_naming_an_open_transaction() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, true), now)?;
    let execute =
        |transaction| Execute::new(1, transaction, 0, TermTable::new(), Vec::new(), Vec::new());
    assert_eq!(
        connection.receive(&peer.send(2, &execute(1))?, now)?,
        Inbound::Request {
            stream: 2,
            command: Command::Execute
        }
    );
    assert_eq!(connection.outbound().credit(2), Some(2));
    assert_eq!(
        connection.receive(&peer.send(3, &execute(7))?, now)?,
        Inbound::Refused {
            stream: 3,
            code: ErrorCode::Protocol
        }
    );
    // An Execute on the transaction's own stream, already open, closes the connection.
    assert_eq!(
        connection.receive(&peer.send(1, &execute(1))?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

#[test]
fn test_requests_of_one_reply_each_open_a_stream_of_their_own() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
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
    assert_eq!(
        connection.receive(&peer.send(1, &prepare)?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Prepare
        }
    );
    assert_eq!(
        connection.receive(&peer.send(2, &Describe::request(0))?, now)?,
        Inbound::Request {
            stream: 2,
            command: Command::Describe
        }
    );
    assert_eq!(
        connection.receive(&peer.send(3, &DropCommand::whole_tenant(0))?, now)?,
        Inbound::Request {
            stream: 3,
            command: Command::Drop
        }
    );
    assert_eq!(connection.outbound().open_streams(), 3);
    assert!(!connection.outbound().is_transaction(1));
    assert!(!connection.outbound().is_transaction(3));
    Ok(())
}

#[test]
fn test_a_transactions_stream_takes_its_operations_and_its_commit() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    let mut terms = TermTable::new();
    let (subject, predicate) = (terms.iri("http://ex.org/c"), terms.iri("http://ex.org/n"));
    let operation = Operation::new(
        terms,
        CommitOperation::CounterAdd {
            graph: None,
            subject,
            predicate,
            delta: 1,
        },
    );
    assert_eq!(
        connection.receive(&peer.send(1, &operation)?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Operation
        }
    );
    // An Operation or an Abort on a stream holding no transaction is refused on it.
    assert_eq!(
        connection.receive(&peer.send(2, &operation)?, now)?,
        Inbound::Refused {
            stream: 2,
            code: ErrorCode::Protocol
        }
    );
    assert_eq!(
        connection.receive(&peer.send(4, &Abort)?, now)?,
        Inbound::Refused {
            stream: 4,
            code: ErrorCode::Protocol
        }
    );
    assert_eq!(
        connection.receive(&peer.send(1, &Commit::new(9u128.to_be_bytes(), 1, 0))?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Commit
        }
    );
    assert!(
        connection.outbound().is_transaction(1),
        "the stream stays its transaction's until the reply leaves"
    );
    // A Commit on an open stream holding no transaction is a new request on an open stream: a connection error.
    request(
        &mut connection,
        &mut peer,
        5,
        &Register::new(0u128.to_be_bytes()),
        now,
    )?;
    assert_eq!(
        connection.receive(&peer.send(5, &Commit::new(9u128.to_be_bytes(), 2, 0))?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

#[test]
fn test_a_refusal_of_a_stream_not_open_leaves_with_the_connections_frames() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    connection.refuse(9, &ErrorReply::new(ErrorCode::Busy, "busy"))?;
    let out = sent(&mut connection)?;
    assert_eq!(out.len(), 1);
    assert_eq!(
        (out[0].0, out[0].1, out[0].2),
        (9, Command::Error.number(), FLAG_LAST)
    );
    assert_eq!(error(&out[0].3)?, (ErrorCode::Busy, 0));
    assert!(!connection.outbound().is_open(9));
    Ok(())
}

#[test]
fn test_renewing_with_a_credential_that_never_expires_clears_the_expiry() -> TestResult {
    // Dave is carol's principal, 9, with a credential that never expires.
    let dave = User {
        name: "dave",
        password: "pw",
        principal: 9,
        valid_for: None,
    };
    let now = start();
    let mut peer = Peer::default();
    let mut connection = ServerConnection::new(
        CLUSTER,
        server(&[TEST]),
        TestAuthenticator::new(vec![CAROL, dave]),
        now,
    );
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    drain(&mut connection)?;
    assert_eq!(
        exchange(&mut connection, &mut peer, "carol", "pw", now)?,
        Inbound::Authenticated { principal: 9 }
    );
    assert_eq!(
        connection.next_deadline(),
        Some(now + Duration::from_secs(60))
    );
    drain(&mut connection)?;
    let later = now + Duration::from_secs(10);
    assert_eq!(
        exchange(&mut connection, &mut peer, "dave", "pw", later)?,
        Inbound::Authenticated { principal: 9 }
    );
    let out = sent(&mut connection)?;
    let view = FrameView::validate(&out[0].3, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(AuthenticateView::from_frame(&view)?.expires(), 0);
    assert_eq!(connection.next_deadline(), Some(later + IDLE_LIMIT));
    assert_eq!(
        connection.handle_timeout(now + Duration::from_secs(61))?,
        None,
        "the old expiry no longer applies"
    );
    Ok(())
}

#[test]
fn test_every_role_command_is_refused_as_not_served_and_closes_the_connection() -> TestResult {
    let now = start();
    let roles: Vec<Command> = Command::ALL
        .into_iter()
        .filter(|command| (512..=767).contains(&command.number()))
        .collect();
    assert_eq!(roles.len(), 20, "Join (512) to Held (531)");
    for role in roles {
        let mut peer = Peer::default();
        let mut connection = serving(&mut peer, now)?;
        assert_eq!(
            connection.receive(&peer.command(1, role)?, now)?,
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "{role:?}"
        );
        assert!(connection.is_closing(), "{role:?}");
        let frames = sent(&mut connection)?;
        let [(stream, command, flags, frame)] = frames.as_slice() else {
            return Err(TestError::new(format!(
                "{role:?}: one closing Error, not {}",
                frames.len()
            )));
        };
        assert_eq!(
            (*stream, *command, *flags),
            (0, Command::Error.number(), FLAG_LAST)
        );
        let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
        let refusal = ErrorReplyView::from_frame(&view)?;
        assert_eq!(refusal.code(), ErrorCode::Protocol, "{role:?}");
        assert_eq!(
            refusal.message(),
            ultragraphdb_error_protocol::ProtocolError::command_not_served(role.number())
                .to_string(),
            "{role:?}"
        );
        assert_eq!(
            connection.receive(&peer.send(3, &Credit::new(1))?, now)?,
            Inbound::Closing { code: None }
        );
    }
    Ok(())
}

#[test]
fn test_limits_past_what_an_instant_holds_set_no_deadline() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[TEST], now).with_limits(Duration::MAX, Duration::MAX);
    assert_eq!(connection.next_deadline(), None, "authenticating");
    assert_eq!(
        connection.handle_timeout(now + AUTHENTICATION_TIMEOUT)?,
        None
    );
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Welcomed {
            protocol: 1,
            max_frame: MAX_FRAME_SIZE as u32
        }
    );
    drain(&mut connection)?;
    assert_eq!(
        exchange(&mut connection, &mut peer, "alice", "pw", now)?,
        Inbound::Authenticated { principal: 7 }
    );
    assert_eq!(connection.next_deadline(), None, "serving, with no expiry");
    assert_eq!(connection.handle_timeout(now + IDLE_LIMIT)?, None);
    assert!(!connection.is_closing());
    Ok(())
}

/// Whether `error` is the frame of `command` that could not be built, past the largest frame.
fn not_built(error: &TransportError, command: Command) -> bool {
    matches!(
        &error.0,
        TransportErrorEnum::FrameNotBuilt {
            command: found,
            error: ProtocolError(ProtocolErrorEnum::FrameTooLarge { size, limit }),
        } if *found == command.number() && *limit == MAX_FRAME_SIZE && *size > MAX_FRAME_SIZE
    )
}

#[test]
fn test_a_welcome_too_large_to_frame_is_an_error_and_sends_nothing() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = ServerConnection::new(
        CLUSTER,
        Welcome::new(1, 0, "s".repeat(MAX_FRAME_SIZE)),
        TestAuthenticator::default(),
        now,
    );
    let Err(error) = connection.receive(&peer.send(0, &hello())?, now) else {
        return Err(TestError::new("a Welcome past the largest frame was taken"));
    };
    assert!(not_built(&error, Command::Welcome), "{error:?}");
    assert!(connection.is_closing());
    assert_eq!(connection.welcome(), None);
    assert_eq!(connection.principal(), None);
    assert_eq!(connection.next_frame(), None, "no frame could be built");
    // A closing connection reads nothing more.
    assert_eq!(
        connection.receive(&peer.send(0, &hello())?, now)?,
        Inbound::Closing { code: None }
    );
    Ok(())
}

#[test]
fn test_a_challenge_too_large_to_frame_is_an_error_and_ends_the_exchange() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    let first = Authenticate::new(0, TEST, format!("user={GIANT_CHALLENGE}").into_bytes());
    let Err(error) = connection.receive(&peer.send(0, &first)?, now) else {
        return Err(TestError::new(
            "a challenge past the largest frame was sent",
        ));
    };
    assert!(not_built(&error, Command::Authenticate), "{error:?}");
    assert!(connection.is_closing());
    assert_eq!(connection.principal(), None);
    assert_eq!(connection.next_frame(), None, "nothing follows the Welcome");
    assert_eq!(connection.next_deadline(), None);
    Ok(())
}

#[test]
fn test_a_final_message_too_large_to_frame_authenticates_no_one() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now)?;
    let first = Authenticate::new(0, TEST, format!("user={GIANT_WELCOME}").into_bytes());
    assert_eq!(
        connection.receive(&peer.send(0, &first)?, now)?,
        Inbound::Handled
    );
    drain(&mut connection)?;
    let proof = Authenticate::new(2, "", b"pass=pw".to_vec());
    let Err(error) = connection.receive(&peer.send(0, &proof)?, now) else {
        return Err(TestError::new(
            "a final message past the largest frame was sent",
        ));
    };
    assert!(not_built(&error, Command::Authenticate), "{error:?}");
    assert_eq!(connection.principal(), None, "no one is authenticated");
    assert!(connection.is_closing());
    assert_eq!(connection.next_frame(), None, "no final message is sent");
    Ok(())
}

#[test]
fn test_a_renewal_too_large_to_frame_keeps_the_credential_held_and_closes() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    // Alice's credential never expires, so the open connection's next deadline is its idle limit.
    let mut connection = serving(&mut peer, now)?;
    let first = Authenticate::new(0, TEST, format!("user={GIANT_WELCOME}").into_bytes());
    assert_eq!(
        connection.receive(&peer.send(0, &first)?, now)?,
        Inbound::Handled
    );
    drain(&mut connection)?;
    let proof = Authenticate::new(2, "", b"pass=pw".to_vec());
    let Err(error) = connection.receive(&peer.send(0, &proof)?, now) else {
        return Err(TestError::new(
            "a final message past the largest frame was sent",
        ));
    };
    assert!(not_built(&error, Command::Authenticate), "{error:?}");
    // The renewal recorded nothing: the principal is alice's, and no expiry was taken from the giant's minute.
    assert_eq!(connection.principal(), Some(7));
    assert!(connection.is_closing());
    assert_eq!(connection.next_deadline(), None);
    assert_eq!(connection.next_frame(), None);
    Ok(())
}

#[test]
fn test_a_refusal_too_large_to_frame_is_an_error_and_leaves_its_stream_open() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
    let Err(error) = connection.refuse(
        1,
        &ErrorReply::new(ErrorCode::Refused, "r".repeat(MAX_FRAME_SIZE)),
    ) else {
        return Err(TestError::new(
            "a refusal past the largest frame was queued",
        ));
    };
    assert!(not_built(&error, Command::Error), "{error:?}");
    assert!(
        connection.outbound().is_open(1),
        "the stream stays as it was"
    );
    assert_eq!(connection.outbound().queued(1), 0);
    assert!(!connection.is_closing());
    // A refusal that frames still ends the stream.
    connection.refuse(1, &ErrorReply::new(ErrorCode::Refused, "no"))?;
    assert_eq!(connection.outbound().queued(1), 1);
    Ok(())
}

#[test]
fn test_a_closing_error_too_large_to_frame_is_an_error_and_the_connection_closes() -> TestResult {
    let now = start();
    let mut connection = connection(&[TEST], now);
    // A rule whose text is past the largest frame: the closing Error that would carry it cannot be built.
    let field: &'static str = "f".repeat(MAX_FRAME_SIZE).leak();
    let Err(error) = connection.refuse_connection(&ProtocolError::field_value(1, field, 0)) else {
        return Err(TestError::new(
            "a closing Error past the largest frame was queued",
        ));
    };
    assert!(not_built(&error, Command::Error), "{error:?}");
    assert!(connection.is_closing());
    assert_eq!(connection.next_frame(), None);
    assert_eq!(connection.handle_timeout(now + IDLE_LIMIT)?, None);
    Ok(())
}

/// What a connection of `endpoint` makes of `command` arriving on a stream of its own.
fn taken_at(endpoint: Endpoint, command: Command) -> TestResult<Inbound> {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?.with_endpoint(endpoint);
    Ok(connection.receive(&peer.command(1, command)?, now)?)
}

/// A role endpoint hands on what it serves and refuses every other request, as the client endpoint refuses a role command.
#[test]
fn test_a_role_endpoint_takes_what_it_serves_and_refuses_the_rest() -> TestResult {
    for (endpoint, served, refused) in [
        (Endpoint::Frontend, Command::Join, Command::Begin),
        (Endpoint::Frontend, Command::Members, Command::Scan),
        (Endpoint::Partition, Command::Scan, Command::Join),
        (Endpoint::Partition, Command::Reference, Command::Begin),
    ] {
        assert_eq!(
            taken_at(endpoint, served)?,
            Inbound::Request {
                stream: 1,
                command: served
            },
            "{endpoint:?}"
        );
        assert_eq!(
            taken_at(endpoint, refused)?,
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "{endpoint:?} refuses {refused:?}"
        );
    }
    Ok(())
}

/// The compute endpoint takes a ComputeRead, its stream starting with the credit it names in place of the Welcome's, and
/// refuses what breaks its body on its stream and another role command as a command not served.
#[test]
fn test_a_compute_read_opens_its_stream_with_its_credit() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?.with_endpoint(Endpoint::Compute);
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let credit = connection.welcome().context("a Welcome")?.initial_credit() + 3;
    let read = ComputeRead::new(1, true, ReadLevel::Snapshot, terms, pattern, Vec::new())
        .with_client_limits(MAX_FRAME_SIZE as u32, credit);
    assert_eq!(
        connection.receive(&peer.send(1, &read)?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::ComputeRead
        }
    );
    assert_eq!(connection.outbound().credit(1), Some(credit));
    assert!(matches!(
        connection.receive(&peer.command(3, Command::ComputeRead)?, now)?,
        Inbound::Refused {
            stream: 3,
            code: ErrorCode::Protocol
        }
    ));
    assert_eq!(connection.outbound().credit(3), None);
    assert_eq!(
        connection.receive(&peer.command(5, Command::Join)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

/// A Query reads as a Read does: on a stream of its own for the transaction it names, which must be open.
#[test]
fn test_a_query_opens_a_stream_for_the_transaction_it_names() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    let _begun: Inbound = connection.receive(&peer.send(1, &Begin::new(0, 0, true))?, now)?;
    let query = |transaction| Query::new(transaction, Language::Gql, "RETURN 1", TermTable::new());
    assert_eq!(
        connection.receive(&peer.send(3, &query(1))?, now)?,
        Inbound::Request {
            stream: 3,
            command: Command::Query
        }
    );
    assert!(matches!(
        connection.receive(&peer.send(5, &query(9))?, now)?,
        Inbound::Refused {
            stream: 5,
            code: ErrorCode::Protocol
        }
    ));
    // Its body refused on its stream.
    assert!(matches!(
        connection.receive(&peer.command(7, Command::Query)?, now)?,
        Inbound::Refused {
            stream: 7,
            code: ErrorCode::Protocol
        }
    ));
    Ok(())
}

/// An Install opens a stream of its own, as a Describe does; a Held is no request.
#[test]
fn test_an_install_opens_a_stream_and_a_held_closes_the_connection() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?;
    let sources = vec![("s".to_string(), "t".to_string())];
    assert_eq!(
        connection.receive(&peer.send(1, &Install::new(0, 0, sources))?, now)?,
        Inbound::Request {
            stream: 1,
            command: Command::Install
        }
    );
    let mut role = Peer::default();
    let mut frontend = serving(&mut role, now)?.with_endpoint(Endpoint::Frontend);
    assert_eq!(
        frontend.receive(&role.send(1, &Held)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

/// A role command stays on the stream it opened: a second on that stream closes the connection, as a Begin's would.
#[test]
fn test_a_role_command_opens_a_stream_of_its_own() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?.with_endpoint(Endpoint::Frontend);
    let _first: Inbound = connection.receive(&peer.command(1, Command::Join)?, now)?;
    assert_eq!(
        connection.receive(&peer.command(1, Command::Members)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

/// `body`, a write in one frame, on stream 1 of a new connection at `endpoint`: it opens a stream of its own, needing no
/// Begin or Bind and holding no transaction; a second request on that stream closes the connection; its reply ends the
/// stream; and its retry goes on a new stream, a request of its own.
fn a_write_in_one_frame_opens_a_stream_of_its_own<B: BodyBuilder>(
    endpoint: Endpoint,
    body: &B,
) -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?.with_endpoint(endpoint);
    request(&mut connection, &mut peer, 1, body, now)?;
    assert!(!connection.outbound().is_transaction(1));
    let reply = Commit::new([0; 16], 0, 0).frame(
        Header::new(CLUSTER, Command::Commit)
            .with_stream(1)
            .with_flags(FLAG_LAST),
    )?;
    connection.outbound().push(1, reply)?;
    drain(&mut connection)?;
    assert_eq!(connection.outbound().open_streams(), 0);
    request(&mut connection, &mut peer, 3, body, now)?;
    assert_eq!(
        connection.receive(&peer.send(3, body)?, now)?,
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    Ok(())
}

/// One DirectWrite inserting a statement.
fn direct_write() -> DirectWrite {
    let mut terms = TermTable::new();
    let (subject, predicate) = (terms.iri("http://ex.org/a"), terms.iri("http://ex.org/n"));
    DirectWrite::new(
        0,
        terms,
        vec![WriteChange::Insert {
            graph: None,
            subject,
            predicate,
            object: subject,
        }],
    )
}

/// One ConditionalWrite adding to a counter.
fn conditional_write() -> ConditionalWrite {
    let mut terms = TermTable::new();
    let (subject, predicate) = (terms.iri("http://ex.org/c"), terms.iri("http://ex.org/n"));
    ConditionalWrite::new(
        0,
        terms,
        Vec::new(),
        vec![CommitOperation::CounterAdd {
            graph: None,
            subject,
            predicate,
            delta: 1,
        }],
    )
}

#[test]
fn test_a_direct_write_opens_a_stream_of_its_own_at_either_endpoint() -> TestResult {
    a_write_in_one_frame_opens_a_stream_of_its_own(Endpoint::Client, &direct_write())?;
    a_write_in_one_frame_opens_a_stream_of_its_own(Endpoint::Partition, &direct_write())
}

#[test]
fn test_a_conditional_write_opens_a_stream_of_its_own_at_either_endpoint() -> TestResult {
    a_write_in_one_frame_opens_a_stream_of_its_own(Endpoint::Client, &conditional_write())?;
    a_write_in_one_frame_opens_a_stream_of_its_own(Endpoint::Partition, &conditional_write())
}

/// A write in one frame is a new request: on a transaction's open stream it closes the connection, as on any open stream.
#[test]
fn test_a_write_in_one_frame_on_a_transactions_stream_closes_the_connection() -> TestResult {
    for command in [Command::DirectWrite, Command::ConditionalWrite] {
        let now = start();
        let mut peer = Peer::default();
        let mut connection = serving(&mut peer, now)?;
        request(&mut connection, &mut peer, 1, &Begin::new(0, 0, false), now)?;
        assert_eq!(
            connection.receive(&peer.command(1, command)?, now)?,
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "{command:?}"
        );
    }
    Ok(())
}

/// A partition answers a Scan with no more Entries frames than compute's credit allows; a Credit on the stream lets the
/// next ones leave (`PROTOCOL.md` §11.3, §13.14).
#[test]
fn test_a_partitions_entries_frames_wait_for_computes_credit() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?.with_endpoint(Endpoint::Partition);
    let mut scan = Scan::new(ScanAt::Latest, 0, 0, 1, [0; 16], 0, 0);
    scan.push_range(ScanRange::new(1, b"a", b""));
    request(&mut connection, &mut peer, 1, &scan, now)?;
    let credit = connection.outbound().credit(1).context("an open stream")?;
    let batches = credit + 1;
    for range in 0..batches {
        let frame = Entries::new(range, EntriesEnd::Ended, 0).frame(
            Header::new(CLUSTER, Command::Entries)
                .with_stream(1)
                .with_flags(if range + 1 == batches { FLAG_LAST } else { 0 }),
        )?;
        connection.outbound().push(1, frame)?;
    }
    let frames = |sent: Frames| sent.iter().filter(|frame| frame.0 == 1).count();
    assert_eq!(frames(sent(&mut connection)?), credit as usize);
    assert_eq!(connection.outbound().credit(1), Some(0));
    assert_eq!(
        connection.receive(&peer.send(1, &Credit::new(1))?, now)?,
        Inbound::Handled
    );
    assert_eq!(frames(sent(&mut connection)?), 1);
    assert!(!connection.outbound().is_open(1));
    Ok(())
}

/// The frontend's role address and a compute process's serve neither write: each closes the connection.
#[test]
fn test_no_other_role_endpoint_takes_a_write_in_one_frame() -> TestResult {
    for endpoint in [Endpoint::Frontend, Endpoint::Compute] {
        for command in [Command::DirectWrite, Command::ConditionalWrite] {
            assert_eq!(
                taken_at(endpoint, command)?,
                Inbound::Closing {
                    code: Some(ErrorCode::Protocol)
                },
                "{endpoint:?} refuses {command:?}"
            );
        }
    }
    Ok(())
}

/// A Bind makes its stream a transaction's; Write, Commit and Drop arrive on it, and neither a Commit nor a Drop opens a
/// stream at a partition.
#[test]
fn test_a_bind_opens_the_stream_a_partition_takes_a_transaction_on() -> TestResult {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now)?.with_endpoint(Endpoint::Partition);
    let _bound: Inbound = connection.receive(&peer.command(1, Command::Bind)?, now)?;
    assert!(connection.outbound().is_transaction(1));
    for command in [
        Command::Write,
        Command::Operation,
        Command::Commit,
        Command::Drop,
    ] {
        assert_eq!(
            connection.receive(&peer.command(1, command)?, now)?,
            Inbound::Request { stream: 1, command },
            "{command:?}"
        );
    }
    for (stream, command) in [(3, Command::Commit), (5, Command::Drop)] {
        assert_eq!(
            connection.receive(&peer.command(stream, command)?, now)?,
            Inbound::Refused {
                stream,
                code: ErrorCode::Protocol
            },
            "{command:?} without a Bind"
        );
    }
    Ok(())
}
