/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use ultragraphdb_protocol_transport::{
    AUTHENTICATION_TIMEOUT, IDLE_LIMIT, Inbound, ServerConnection,
};
use ultragraphdb_protocol_wire::{
    Abort, Authenticate, AuthenticateView, Begin, BodyBuilder, BodyView, Cancel, Command, Commit,
    Credit, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST, FrameView, Header, HeaderView, Hello,
    MAX_FRAME_SIZE, PatternSlot, QuadPattern, Read, Register, ResultBatch, TermTable, Welcome,
    WelcomeView, Write,
};

use crate::utils::mechanism::{EXPIRES, TEST, TestAuthenticator, User};
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
        TestAuthenticator::new(vec![ALICE, BOB, CAROL]),
        now,
    )
}

/// The frames the connection sends, as (stream, command, flags, bytes).
fn sent(connection: &mut Connection) -> Vec<(u64, u16, u8, Vec<u8>)> {
    std::iter::from_fn(|| connection.next_frame())
        .map(|(stream, frame)| {
            let header = HeaderView::new(&frame).unwrap();
            (stream, header.command(), header.flags(), frame)
        })
        .collect()
}

/// The code and detail of an Error frame.
fn error(frame: &[u8]) -> (ErrorCode, u64) {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let error = ErrorReplyView::from_frame(&view).unwrap();
    (error.code(), error.detail())
}

fn hello() -> Hello {
    Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "client")
}

/// A connection past its Hello.
fn welcomed(mechanisms: &[&str], peer: &mut Peer, now: Instant) -> Connection {
    let mut connection = connection(mechanisms, now);
    assert_eq!(
        connection.receive(&peer.send(0, &hello()), now),
        Inbound::Welcomed { protocol: 1 }
    );
    sent(&mut connection);
    connection
}

/// `name`'s exchange with `password` on `connection`; what its final step made of it.
fn exchange(
    connection: &mut Connection,
    peer: &mut Peer,
    name: &str,
    password: &str,
    now: Instant,
) -> Inbound {
    let first = Authenticate::new(0, TEST, format!("user={name}").into_bytes());
    assert_eq!(
        connection.receive(&peer.send(0, &first), now),
        Inbound::Handled
    );
    // The challenge on stream 0; a read may stream beside it.
    let frames = sent(connection);
    let challenge: Vec<_> = frames.iter().filter(|f| f.0 == 0).collect();
    assert_eq!(challenge.len(), 1);
    let view = FrameView::validate(&challenge[0].3, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let message = AuthenticateView::from_frame(&view).unwrap();
    assert_eq!((message.step(), message.message()), (1, &b"challenge"[..]));
    let proof = Authenticate::new(2, "", format!("pass={password}").into_bytes());
    connection.receive(&peer.send(0, &proof), now)
}

/// A connection authenticated as alice.
fn serving(peer: &mut Peer, now: Instant) -> Connection {
    let mut connection = welcomed(&[TEST], peer, now);
    assert_eq!(
        exchange(&mut connection, peer, "alice", "pw", now),
        Inbound::Authenticated { principal: 7 }
    );
    sent(&mut connection);
    connection
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

fn result(stream: u64, last: bool) -> Vec<u8> {
    ResultBatch::new(1)
        .frame(
            Header::new(CLUSTER, Command::Result)
                .with_stream(stream)
                .with_flags(if last { FLAG_LAST } else { 0 }),
        )
        .unwrap()
}

#[test]
fn test_a_hello_is_welcomed_with_the_servers_mechanisms_at_sequence_one() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[TEST], now);
    assert!(connection.welcome().is_none());
    assert_eq!(
        connection.receive(&peer.send(0, &hello()), now),
        Inbound::Welcomed { protocol: 1 }
    );
    let frames = sent(&mut connection);
    assert_eq!(frames.len(), 1);
    let (stream, command, _, bytes) = &frames[0];
    assert_eq!((*stream, *command), (0, Command::Welcome.number()));
    assert_eq!(HeaderView::new(bytes).unwrap().sequence(), 1);
    let view = FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let welcome = WelcomeView::from_frame(&view).unwrap();
    assert!(welcome.authenticates());
    assert_eq!(welcome.mechanisms().iter().collect::<Vec<_>>(), [TEST]);
    assert_eq!(connection.welcome().map(Welcome::protocol), Some(1));
    assert_eq!(connection.principal(), None, "not yet authenticated");
}

#[test]
fn test_a_first_frame_other_than_hello_closes_with_a_protocol_error() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[TEST], now);
    assert_eq!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now),
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let frames = sent(&mut connection);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0, 0, "on stream 0");
    assert_eq!(error(&frames[0].3).0, ErrorCode::Protocol);
    assert!(connection.is_closing());
}

#[test]
fn test_an_exchange_authenticates_the_principal_its_final_message_names() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now);
    assert_eq!(
        exchange(&mut connection, &mut peer, "alice", "pw", now),
        Inbound::Authenticated { principal: 7 }
    );
    let frames = sent(&mut connection);
    let view = FrameView::validate(&frames[0].3, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let last = AuthenticateView::from_frame(&view).unwrap();
    assert!(last.is_final());
    assert_eq!((last.step(), last.principal(), last.expires()), (3, 7, 0));
    assert_eq!(last.message(), b"welcome");
    assert_eq!(connection.principal(), Some(7));
}

#[test]
fn test_a_failed_exchange_closes_with_code_19_saying_nothing_of_why() {
    let now = start();
    // A wrong password and a name no principal holds give the same frame.
    let mut frames = Vec::new();
    for (name, password) in [("alice", "wrong"), ("nobody", "pw")] {
        let mut peer = Peer::default();
        let mut connection = welcomed(&[TEST], &mut peer, now);
        assert_eq!(
            exchange(&mut connection, &mut peer, name, password, now),
            Inbound::Closing {
                code: Some(ErrorCode::AuthenticationFailed)
            }
        );
        let out = sent(&mut connection);
        assert_eq!(out.len(), 1);
        assert_eq!(error(&out[0].3), (ErrorCode::AuthenticationFailed, 0));
        assert!(connection.is_closing());
        frames.push(out[0].3.clone());
    }
    assert_eq!(frames[0], frames[1], "the two refusals are the same bytes");
    // A mechanism the server does not offer.
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now);
    let other = Authenticate::new(0, "PLAIN", b"x".to_vec());
    assert_eq!(
        connection.receive(&peer.send(0, &other), now),
        Inbound::Closing {
            code: Some(ErrorCode::AuthenticationFailed)
        }
    );
}

#[test]
fn test_a_request_before_authentication_closes_with_code_20() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now);
    assert_eq!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now),
        Inbound::Closing {
            code: Some(ErrorCode::Unauthenticated)
        }
    );
    let out = sent(&mut connection);
    assert_eq!(
        (out[0].0, error(&out[0].3).0),
        (0, ErrorCode::Unauthenticated)
    );
}

#[test]
fn test_an_unfinished_authentication_times_out_with_code_20() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now);
    assert_eq!(
        connection.next_deadline(),
        Some(now + AUTHENTICATION_TIMEOUT)
    );
    assert_eq!(
        connection.handle_timeout(now + AUTHENTICATION_TIMEOUT - Duration::from_millis(1)),
        None
    );
    assert_eq!(
        connection.handle_timeout(now + AUTHENTICATION_TIMEOUT),
        Some(Inbound::Closing {
            code: Some(ErrorCode::Unauthenticated)
        })
    );
    let out = sent(&mut connection);
    assert_eq!(error(&out[0].3).0, ErrorCode::Unauthenticated);
    assert_eq!(
        connection.next_deadline(),
        None,
        "a closing connection has no deadline"
    );
    // An authenticated connection is past the timeout: its next deadline is the idle limit.
    let mut peer = Peer::default();
    let connection = serving(&mut peer, now);
    assert_eq!(connection.next_deadline(), Some(now + IDLE_LIMIT));
}

#[test]
fn test_a_server_without_authentication_serves_at_once() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[], now);
    assert_eq!(
        connection.receive(&peer.send(0, &hello()), now),
        Inbound::Welcomed { protocol: 1 }
    );
    let out = sent(&mut connection);
    let view = FrameView::validate(&out[0].3, CLUSTER, MAX_FRAME_SIZE).unwrap();
    assert!(!WelcomeView::from_frame(&view).unwrap().authenticates());
    assert_eq!(connection.principal(), Some(0), "the anonymous principal");
    assert_eq!(
        connection.receive(&peer.send(1, &Register::new(0)), now),
        Inbound::Request {
            stream: 1,
            command: Command::Register
        }
    );
    // An exchange where none is offered is a protocol error.
    let first = Authenticate::new(0, TEST, b"user=alice".to_vec());
    assert_eq!(
        connection.receive(&peer.send(0, &first), now),
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
}

#[test]
fn test_a_credential_is_renewed_on_the_open_connection_while_a_read_streams() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now);
    assert_eq!(
        exchange(&mut connection, &mut peer, "carol", "pw", now),
        Inbound::Authenticated { principal: 9 }
    );
    let out = sent(&mut connection);
    let view = FrameView::validate(&out[0].3, CLUSTER, MAX_FRAME_SIZE).unwrap();
    assert_eq!(
        AuthenticateView::from_frame(&view).unwrap().expires(),
        EXPIRES
    );
    assert_eq!(
        connection.next_deadline(),
        Some(now + Duration::from_secs(60))
    );
    // A transaction and a read streaming.
    assert!(matches!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now),
        Inbound::Request { .. }
    ));
    assert_eq!(
        connection.receive(&peer.send(3, &read(1)), now),
        Inbound::Request {
            stream: 3,
            command: Command::Read
        }
    );
    connection.outbound().push(3, result(3, false)).unwrap();
    // Renewed 50 seconds in, the deadline moves to a minute from the renewal.
    let later = now + Duration::from_secs(50);
    assert_eq!(
        exchange(&mut connection, &mut peer, "carol", "pw", later),
        Inbound::Authenticated { principal: 9 }
    );
    assert_eq!(
        connection.next_deadline(),
        Some(later + Duration::from_secs(60))
    );
    assert_eq!(
        connection.handle_timeout(now + Duration::from_secs(61)),
        None
    );
    // The read is open past the first expiry and goes on streaming.
    connection.outbound().push(3, result(3, true)).unwrap();
    let streams: Vec<u64> = sent(&mut connection).iter().map(|f| f.0).collect();
    assert_eq!(
        streams,
        vec![0, 3],
        "the final Authenticate, then the read, going on after the renewal"
    );
}

#[test]
fn test_renewing_as_another_principal_is_refused_with_code_19() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    assert_eq!(
        exchange(&mut connection, &mut peer, "bob", "pw", now),
        Inbound::Closing {
            code: Some(ErrorCode::AuthenticationFailed)
        }
    );
}

#[test]
fn test_an_expired_credential_closes_with_code_22() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now);
    exchange(&mut connection, &mut peer, "carol", "pw", now);
    sent(&mut connection);
    assert_eq!(
        connection.handle_timeout(now + Duration::from_secs(60)),
        Some(Inbound::Closing {
            code: Some(ErrorCode::CredentialExpired)
        })
    );
    let out = sent(&mut connection);
    assert_eq!(
        (out[0].0, error(&out[0].3).0),
        (0, ErrorCode::CredentialExpired)
    );
}

#[test]
fn test_an_idle_connection_closes_without_a_frame() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    // A frame resets the idle limit.
    let later = now + Duration::from_secs(300);
    connection.receive(&peer.send(1, &Register::new(0)), later);
    assert_eq!(connection.handle_timeout(now + IDLE_LIMIT), None);
    assert_eq!(
        connection.handle_timeout(later + IDLE_LIMIT),
        Some(Inbound::Closing { code: None })
    );
    assert!(sent(&mut connection).is_empty());
    assert!(connection.is_closing());
}

#[test]
fn test_a_request_on_stream_zero_or_on_an_open_stream_closes_the_connection() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    assert_eq!(
        connection.receive(&peer.send(0, &Begin::new(0, 0, false)), now),
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let out = sent(&mut connection);
    assert_eq!((out[0].0, error(&out[0].3).0), (0, ErrorCode::Protocol));
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now);
    assert_eq!(
        connection.receive(&peer.send(1, &Register::new(0)), now),
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
}

#[test]
fn test_a_stream_past_the_limit_is_refused_on_it_and_the_connection_stays() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    for stream in 1..=4 {
        connection.receive(&peer.send(stream, &Begin::new(0, 0, false)), now);
    }
    assert_eq!(
        connection.receive(&peer.send(5, &Register::new(0)), now),
        Inbound::Refused {
            stream: 5,
            code: ErrorCode::StreamLimit
        }
    );
    let out = sent(&mut connection);
    let refusal = out.iter().find(|f| f.0 == 5).unwrap();
    assert_eq!(error(&refusal.3), (ErrorCode::StreamLimit, 4));
    assert_eq!(refusal.2, FLAG_LAST);
    assert!(!connection.is_closing());
}

#[test]
fn test_a_read_runs_on_its_own_stream_naming_an_open_transaction() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    assert_eq!(
        connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now),
        Inbound::Request {
            stream: 1,
            command: Command::Begin
        }
    );
    assert!(connection.outbound().is_transaction(1));
    assert_eq!(
        connection.receive(&peer.send(2, &read(1)), now),
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
        connection.receive(&peer.send(3, &read(9)), now),
        Inbound::Refused {
            stream: 3,
            code: ErrorCode::Protocol
        }
    );
    assert!(!connection.is_closing());
}

#[test]
fn test_cancels_end_a_read_alone_or_a_transaction_with_its_reads() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now);
    connection.receive(&peer.send(2, &read(1)), now);
    connection.receive(&peer.send(3, &read(1)), now);
    assert_eq!(
        connection.receive(&peer.send(2, &Cancel), now),
        Inbound::Cancelled {
            stream: 2,
            transaction: false
        }
    );
    assert!(connection.outbound().is_transaction(1));
    assert_eq!(
        connection.receive(&peer.send(1, &Cancel), now),
        Inbound::Cancelled {
            stream: 1,
            transaction: true
        }
    );
    assert!(
        !connection.outbound().is_open(3),
        "the transaction's read ended with it"
    );
}

#[test]
fn test_credit_is_granted_and_writes_go_to_their_transaction() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now);
    connection.receive(&peer.send(2, &read(1)), now);
    assert_eq!(
        connection.receive(&peer.send(2, &Credit::new(5)), now),
        Inbound::Handled
    );
    assert_eq!(connection.outbound().credit(2), Some(7));
    let write = Write::new(TermTable::new(), Vec::new());
    assert_eq!(
        connection.receive(&peer.send(1, &write), now),
        Inbound::Request {
            stream: 1,
            command: Command::Write
        }
    );
    // A Write on a stream holding no transaction is refused on it.
    assert_eq!(
        connection.receive(&peer.send(4, &write), now),
        Inbound::Refused {
            stream: 4,
            code: ErrorCode::Protocol
        }
    );
    // An Abort ends its transaction and is the session's to apply.
    assert_eq!(
        connection.receive(&peer.send(1, &Abort), now),
        Inbound::Request {
            stream: 1,
            command: Command::Abort
        }
    );
    assert!(!connection.outbound().is_open(1));
    assert!(!connection.outbound().is_open(2));
}

#[test]
fn test_a_commit_retried_on_a_new_stream_is_a_request_of_its_own() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    assert_eq!(
        connection.receive(&peer.send(6, &Commit::new(3, 2, 0, Vec::new())), now),
        Inbound::Request {
            stream: 6,
            command: Command::Commit
        }
    );
    assert!(connection.outbound().is_open(6));
    assert!(!connection.outbound().is_transaction(6));
}

#[test]
fn test_the_sessions_refusal_ends_its_stream_and_a_transactions_reads() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now);
    connection.receive(&peer.send(2, &read(1)), now);
    connection.refuse(1, &ErrorReply::new(ErrorCode::Protocol, "a repeated term"));
    let out = sent(&mut connection);
    let refusal = out.iter().find(|f| f.0 == 1).unwrap();
    assert_eq!(refusal.2, FLAG_LAST);
    assert_eq!(error(&refusal.3).0, ErrorCode::Protocol);
    assert!(!connection.outbound().is_open(1));
    assert!(!connection.outbound().is_open(2));
    assert!(!connection.is_closing());
}

#[test]
fn test_a_frame_out_of_sequence_closes_the_connection() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    peer.skip();
    assert_eq!(
        connection.receive(&peer.send(1, &Register::new(0)), now),
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let out = sent(&mut connection);
    assert_eq!((out[0].0, error(&out[0].3).0), (0, ErrorCode::Protocol));
    // Nothing more is taken once closing.
    assert_eq!(
        connection.receive(&peer.send(2, &Register::new(0)), now),
        Inbound::Closing { code: None }
    );
    assert!(sent(&mut connection).is_empty());
}

#[test]
fn test_every_frame_the_connection_sends_is_numbered_in_order() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = welcomed(&[TEST], &mut peer, now);
    let first = Authenticate::new(0, TEST, b"user=alice".to_vec());
    connection.receive(&peer.send(0, &first), now);
    let proof = Authenticate::new(2, "", b"pass=pw".to_vec());
    connection.receive(&peer.send(0, &proof), now);
    // The Welcome was 1; the challenge and the final message follow it.
    let sequences: Vec<u128> = sent(&mut connection)
        .iter()
        .map(|f| HeaderView::new(&f.3).unwrap().sequence())
        .collect();
    assert_eq!(sequences, vec![2, 3]);
}

#[test]
fn test_a_new_request_on_an_open_stream_closes_before_its_body_is_read() {
    // A Read whose body breaks a rule, on a stream already open: the stream rule closes the connection first.
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now);
    connection.receive(&peer.send(3, &Register::new(0)), now);
    let mut bad = read(1)
        .frame(Header::new(CLUSTER, Command::Read).with_stream(3))
        .unwrap();
    // flags bit 1: a Read's body rule, refused on its stream were the stream new.
    bad[128 + 12] = 2;
    peer.skip();
    Header::stamp_sequence(&mut bad, 6).unwrap();
    assert_eq!(
        connection.receive(&peer.raw(bad), now),
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
}

#[test]
fn test_every_failed_exchange_is_refused_with_the_same_bytes() {
    // A wrong password, and a message the mechanism does not take at its step: two failures, one refusal.
    let now = start();
    let refusal = |message: &[u8]| {
        let mut peer = Peer::default();
        let mut connection = welcomed(&[TEST], &mut peer, now);
        let first = Authenticate::new(0, TEST, b"user=alice".to_vec());
        connection.receive(&peer.send(0, &first), now);
        sent(&mut connection);
        let second = Authenticate::new(2, "", message.to_vec());
        assert_eq!(
            connection.receive(&peer.send(0, &second), now),
            Inbound::Closing {
                code: Some(ErrorCode::AuthenticationFailed)
            }
        );
        sent(&mut connection).remove(0).3
    };
    assert_eq!(refusal(b"pass=wrong"), refusal(b"garbage"));
}

#[test]
fn test_every_frame_on_stream_zero_but_hello_and_authenticate_closes_the_connection() {
    let now = start();
    let write = Write::new(TermTable::new(), Vec::new());
    for frame in [0, 1, 2] {
        let mut peer = Peer::default();
        let mut connection = serving(&mut peer, now);
        let received = match frame {
            0 => peer.send(0, &Credit::new(1)),
            1 => peer.send(0, &Cancel),
            _ => peer.send(0, &write),
        };
        assert_eq!(
            connection.receive(&received, now),
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "frame {frame}"
        );
    }
}

#[test]
fn test_a_closing_connection_sends_nothing_after_its_error() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    connection.receive(&peer.send(1, &Begin::new(0, 0, false)), now);
    connection
        .outbound()
        .push(1, {
            Commit::new(1, 1, 2, Vec::new())
                .frame(
                    Header::new(CLUSTER, Command::Commit)
                        .with_stream(1)
                        .with_flags(FLAG_LAST),
                )
                .unwrap()
        })
        .unwrap();
    assert_eq!(
        connection.receive(&peer.send(0, &Begin::new(0, 0, false)), now),
        Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }
    );
    let out = sent(&mut connection);
    assert_eq!(out.len(), 1, "the Begin's reply and the Commit stay unsent");
    assert_eq!((out[0].0, error(&out[0].3).0), (0, ErrorCode::Protocol));
    assert_eq!(connection.next_frame(), None);
}

/// A Hello whose lowest protocol is above the server's highest is answered with code 16 on stream 0, `detail` the
/// server's highest, and the connection closes: nothing more is read or sent (point 7).
#[test]
fn test_a_hello_above_the_servers_protocols_closes_with_code_16() {
    let now = start();
    let mut peer = Peer::default();
    let mut connection = connection(&[], now);
    assert_eq!(
        connection.receive(
            &peer.send(0, &Hello::new(2, 3, 0, MAX_FRAME_SIZE as u32, "client")),
            now
        ),
        Inbound::Closing {
            code: Some(ErrorCode::ProtocolVersion)
        }
    );
    assert!(connection.is_closing());
    let out = sent(&mut connection);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, 0);
    assert_eq!(error(&out[0].3), (ErrorCode::ProtocolVersion, 1));
    assert_eq!(
        connection.receive(&peer.send(1, &Register::new(0)), now),
        Inbound::Closing { code: None },
        "a closing connection reads nothing"
    );
    assert_eq!(connection.next_frame(), None);
}

#[test]
fn test_a_cancel_of_a_stream_not_open_is_handled_and_answered_with_nothing() {
    // A Cancel of a stream the connection holds no longer, or never held, changes nothing (PROTOCOL.md section 11.4).
    let now = start();
    let mut peer = Peer::default();
    let mut connection = serving(&mut peer, now);
    assert_eq!(
        connection.receive(&peer.send(5, &Cancel), now),
        Inbound::Handled
    );
    assert!(sent(&mut connection).is_empty());
    assert!(!connection.is_closing());
}

#[test]
fn test_a_command_only_a_server_sends_closes_the_connection() {
    let now = start();
    for frame in [0, 1, 2] {
        let mut peer = Peer::default();
        let mut connection = serving(&mut peer, now);
        let received = match frame {
            0 => peer.send(1, &ResultBatch::new(1)),
            1 => peer.send(1, &server(&[])),
            _ => peer.send(1, &ErrorReply::new(ErrorCode::Refused, "no")),
        };
        assert_eq!(
            connection.receive(&received, now),
            Inbound::Closing {
                code: Some(ErrorCode::Protocol)
            },
            "frame {frame}"
        );
    }
}
