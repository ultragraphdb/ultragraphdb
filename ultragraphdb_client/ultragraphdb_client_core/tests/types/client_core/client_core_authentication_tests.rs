/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_client_core::{
    BatchPoll, ClientCore, ClientErrorEnum, Reply, TEST_NAME, TEST_PRINCIPAL, TestServer,
};
use ultragraphdb_protocol_scram::{ScramError, ScramLogin};
use ultragraphdb_protocol_transport::{Anonymous, BufferPool, NetworkFaults, TransportError};
use ultragraphdb_protocol_wire::{
    AuthenticateView, BodyBuilder, BodyView, Command, FrameView, Header, HeaderView,
    MAX_FRAME_SIZE, PatternSlot, QuadPattern, Read, TermTable,
};

use crate::utils::harness::{
    CLUSTER, Harness, TestLogin, UNIX, config, login, nonce, received, scram_welcome, start,
};

/// The test server asking for SCRAM-SHA-256.
fn scram_server() -> TestServer {
    TestServer::new(CLUSTER, scram_welcome())
}

/// Every Authenticate the server received: its step and whether it is final.
fn exchanges(h: &Harness<TestLogin>) -> Vec<u32> {
    h.server
        .received()
        .iter()
        .filter_map(|f| {
            let view = FrameView::validate_client(f, CLUSTER, MAX_FRAME_SIZE).ok()?;
            AuthenticateView::from_frame(&view).ok().map(|a| a.step())
        })
        .collect()
}

/// The Welcome of [`scram_welcome`], the connection's first frame.
fn welcome_frame() -> Vec<u8> {
    scram_welcome()
        .frame(Header::new(CLUSTER, Command::Welcome).with_sequence(1))
        .unwrap()
}

/// A Connections authenticate scenario: a client authenticates with SCRAM-SHA-256.
#[test]
fn test_a_client_authenticates_with_scram_and_its_requests_are_served() {
    let mut h = Harness::authenticated(scram_server());
    assert_eq!(h.core.principal(), Some(TEST_PRINCIPAL));
    assert_eq!(h.server.principal(), Some(TEST_PRINCIPAL));
    assert_eq!(h.core.expires(), None, "the credential never expires");
    assert_eq!(
        exchanges(&h),
        vec![0, 2],
        "the client's first and final messages"
    );
    let p = h.core.register(h.now).unwrap();
    assert!(matches!(h.settle(p), Ok(Reply::Registered(_))));
    assert_eq!(h.core.next_deadline(), None, "no renewal is due");
}

#[test]
fn test_requests_wait_for_the_final_authenticate() {
    let pool = BufferPool::new(2, 1 << 20);
    let now = start();
    let mut core = ClientCore::new(config(), login());
    core.connect(now, UNIX).unwrap();
    let _ = core.transmit();
    core.receive(received(&pool, &welcome_frame()).unwrap(), now)
        .unwrap();
    assert!(!core.is_ready(), "authenticating");
    assert_eq!(core.welcome(), Some(&scram_welcome()));
    assert_eq!(
        core.register(now).unwrap_err().0,
        ClientErrorEnum::NotConnected
    );
    let first = core.transmit().expect("the client's first message");
    let header = HeaderView::new(&first).unwrap();
    assert_eq!((header.stream(), header.sequence()), (0, 2));
    let view = FrameView::validate_client(&first, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let message = AuthenticateView::from_frame(&view).unwrap();
    assert_eq!((message.step(), message.mechanism()), (0, "SCRAM-SHA-256"));
    assert_eq!(
        core.next_deadline(),
        Some(now + Duration::from_secs(1)),
        "the exchange is awaited within the configured timeout, 1 s"
    );
}

#[test]
fn test_a_wrong_password_closes_the_connection() {
    let wrong = ScramLogin::new(
        TEST_NAME,
        "wrong",
        nonce as fn() -> Result<String, ScramError>,
    )
    .unwrap();
    let mut h = Harness::with(config(), scram_server(), NetworkFaults::none(), 1, wrong);
    h.connect();
    h.until(|h| !h.core.is_connected());
    assert_eq!(h.core.principal(), None);
    assert_eq!(h.server.principal(), None);
    assert!(h.server.is_closing());
}

#[test]
fn test_a_login_speaking_no_offered_mechanism_gives_the_connection_up() {
    let pool = BufferPool::new(2, 1 << 20);
    let now = start();
    let mut core = ClientCore::new(config(), Anonymous);
    core.connect(now, UNIX).unwrap();
    let _ = core.transmit();
    let refused = core
        .receive(received(&pool, &welcome_frame()).unwrap(), now)
        .unwrap_err();
    assert_eq!(
        refused.0,
        ClientErrorEnum::Transport {
            error: TransportError::Authentication(
                "the login speaks none of the server's mechanisms"
            )
        }
    );
    assert!(!core.is_connected());
}

fn read(tx: u64) -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Read::new(
        tx,
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

/// A Connections authenticate scenario: a credential is renewed on the open connection while a read streams.
#[test]
fn test_a_credential_is_renewed_before_it_expires_while_a_read_streams() {
    let life = Duration::from_secs(600);
    let mut h = Harness::authenticated(scram_server().with_credential_life(life).with_results(6));
    let first = UNIX + life.as_micros() as u64;
    assert_eq!(h.core.expires(), Some(first));
    let p = h.core.begin(0, true, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!("a transaction")
    };
    let stream = h.core.read(&tx, &read(tx.stream()), h.now).unwrap();
    h.until(|h| h.server.received().len() >= 5);
    while h.step() {}
    let renew = h.core.next_deadline().expect("the renewal is due");
    assert_eq!(
        renew,
        h.now + Duration::from_secs(540),
        "60 s before the expiry"
    );
    h.now = renew;
    h.core.handle_timeout(h.now);
    let renewed = UNIX + (Duration::from_secs(540) + life).as_micros() as u64;
    h.until(|h| h.core.expires() == Some(renewed));
    assert_eq!(
        exchanges(&h),
        vec![0, 2, 0, 2],
        "a second exchange on stream 0"
    );
    assert_eq!(h.core.principal(), Some(TEST_PRINCIPAL));
    let mut batches = 0;
    h.until(|h| {
        loop {
            match h.core.next_batch(&stream, h.now, |_| ()) {
                Ok(BatchPoll::Batch(())) => batches += 1,
                Ok(BatchPoll::Done) => return true,
                Ok(BatchPoll::Wait) => return false,
                Err(e) => panic!("{e:?}"),
            }
        }
    });
    assert_eq!(batches, 6, "the read went on through the renewal");
    assert_eq!(h.connections, 1);
}

#[test]
fn test_a_short_credential_is_renewed_at_half_its_life() {
    let h = Harness::authenticated(scram_server().with_credential_life(Duration::from_secs(60)));
    assert_eq!(
        h.core.next_deadline(),
        Some(h.now + Duration::from_secs(30))
    );
}

#[test]
fn test_a_new_connection_forgets_the_principal_until_it_authenticates_again() {
    let mut h = Harness::authenticated(scram_server());
    h.connect();
    assert_eq!(h.core.principal(), None);
    h.until(|h| h.core.is_ready());
    assert_eq!(h.core.principal(), Some(TEST_PRINCIPAL));
    assert_eq!(exchanges(&h), vec![0, 2, 0, 2]);
}

/// A mechanism of the tests, `TEST`: its messages are empty and the server's final message is taken when it reads
/// `ok`, so a test plays the server with frames of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Plain;

impl ultragraphdb_protocol_transport::ClientMechanism for Plain {
    type Error = &'static str;

    fn name(&self) -> &str {
        "TEST"
    }

    fn start(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(Vec::new())
    }

    fn answer(&mut self, _challenge: &[u8]) -> Result<Vec<u8>, &'static str> {
        Ok(Vec::new())
    }

    fn finish(&mut self, outcome: &[u8]) -> Result<(), &'static str> {
        (outcome == b"ok")
            .then_some(())
            .ok_or("the server did not prove itself")
    }
}

impl ultragraphdb_protocol_transport::Login for Plain {
    type Mechanism = Plain;

    fn mechanism(
        &mut self,
        _offered: &[String],
    ) -> Result<Option<Plain>, ultragraphdb_protocol_transport::TransportError> {
        Ok(Some(Plain))
    }
}

/// A core of the [`Plain`] login played against frames of the test's own, each numbered as the connection's next.
struct Played {
    core: ClientCore<Plain>,
    pool: BufferPool,
    framer: ultragraphdb_protocol_transport::Framer,
    sent: u128,
}

impl Played {
    /// Connected, its Welcome asking for `TEST`, its first exchange ended as principal 7 expiring at `expires`.
    fn authenticated(expires: u64) -> Self {
        let mut played = Self {
            core: ClientCore::new(config(), Plain),
            pool: BufferPool::new(4, 1 << 20),
            framer: ultragraphdb_protocol_transport::Framer::new(CLUSTER, MAX_FRAME_SIZE),
            sent: 0,
        };
        played.core.connect(start(), UNIX).unwrap();
        let welcome = crate::utils::harness::welcome()
            .with_mechanisms(["TEST"])
            .frame(Header::new(CLUSTER, Command::Welcome))
            .unwrap();
        played.play(&welcome).unwrap();
        played.play(&Self::last(7, expires)).unwrap();
        assert!(played.core.is_ready());
        played
    }

    /// The server's final Authenticate of the exchange, naming `principal`.
    fn last(principal: u64, expires: u64) -> Vec<u8> {
        ultragraphdb_protocol_wire::Authenticate::new(1, "", b"ok".to_vec())
            .with_final(principal, expires)
            .frame(Header::new(CLUSTER, Command::Authenticate))
            .unwrap()
    }

    /// `frame` delivered as the connection's next.
    fn play(&mut self, frame: &[u8]) -> Result<(), ultragraphdb_client_core::ClientError> {
        self.sent += 1;
        let mut frame = frame.to_vec();
        Header::stamp_sequence(&mut frame, self.sent).unwrap();
        let received =
            crate::utils::harness::frame_through(&mut self.framer, &self.pool, &frame).unwrap();
        self.core.receive(received, start())
    }
}

#[test]
fn test_a_renewal_ending_as_another_principal_gives_the_connection_up() {
    let expires = UNIX + 600_000_000;
    let mut played = Played::authenticated(expires);
    // The Hello and the first exchange's step 0, frames 1 and 2, go first.
    assert_eq!(std::iter::from_fn(|| played.core.transmit()).count(), 2);
    let renew = played.core.next_deadline().unwrap();
    assert_eq!(played.core.transmit(), None, "nothing before the renewal");
    played.core.handle_timeout(renew);
    // The renewal's first message: step 0 on stream 0, naming the mechanism, the connection's frame after the
    // first exchange's step 0.
    let first = played.core.transmit().expect("the renewal's first message");
    let header = HeaderView::new(&first).unwrap();
    assert_eq!((header.stream(), header.sequence()), (0, 3));
    let view = FrameView::validate_client(&first, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let message = AuthenticateView::from_frame(&view).unwrap();
    assert_eq!((message.step(), message.mechanism()), (0, "TEST"));
    let refused = played
        .play(&Played::last(8, expires + 600_000_000))
        .unwrap_err();
    assert_eq!(
        refused.0,
        ClientErrorEnum::Transport {
            error: TransportError::Authentication("renewed as another principal")
        }
    );
    assert!(!played.core.is_connected());
}

#[test]
fn test_a_server_that_does_not_prove_itself_gives_the_connection_up() {
    let played = Played::authenticated(0);
    assert_eq!(played.core.principal(), Some(7));
    assert_eq!(played.core.expires(), None);
    let mut fresh = Played {
        core: ClientCore::new(config(), Plain),
        pool: BufferPool::new(4, 1 << 20),
        framer: ultragraphdb_protocol_transport::Framer::new(CLUSTER, MAX_FRAME_SIZE),
        sent: 0,
    };
    fresh.core.connect(start(), UNIX).unwrap();
    let welcome = crate::utils::harness::welcome()
        .with_mechanisms(["TEST"])
        .frame(Header::new(CLUSTER, Command::Welcome))
        .unwrap();
    fresh.play(&welcome).unwrap();
    let forged = ultragraphdb_protocol_wire::Authenticate::new(1, "", b"no".to_vec())
        .with_final(7, 0)
        .frame(Header::new(CLUSTER, Command::Authenticate))
        .unwrap();
    let refused = fresh.play(&forged).unwrap_err();
    assert_eq!(
        refused.0,
        ClientErrorEnum::Transport {
            error: TransportError::Authentication("the server did not prove itself")
        }
    );
    assert!(!fresh.core.is_connected());
    assert_eq!(fresh.core.principal(), None);
}

#[test]
fn test_a_commit_lost_with_its_connection_is_retried_once_the_next_one_authenticated() {
    let mut h = Harness::authenticated(scram_server());
    let p = h.core.register(h.now).unwrap();
    let Ok(Reply::Registered(session)) = h.settle(p) else {
        panic!("a session")
    };
    let p = h.core.begin(0, false, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!("a transaction")
    };
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, session, h.now).unwrap();
    let Ok(Reply::Committed(committed)) = h.settle(p) else {
        panic!("committed through the retry")
    };
    assert_eq!(committed.request(), 1);
    assert_eq!(h.server.applied(), 1);
    assert_eq!(h.connections, 2);
    assert_eq!(
        exchanges(&h),
        vec![0, 2, 0, 2],
        "the retry waited for the second exchange"
    );
}

/// The second renewal is due its margin before the second expiry, on the wall clock as it has moved.
#[test]
fn test_each_renewal_is_scheduled_from_the_wall_clock_at_its_end() {
    let life = Duration::from_secs(600);
    let mut h = Harness::authenticated(scram_server().with_credential_life(life));
    let first = h.core.next_deadline().unwrap();
    h.now = first;
    h.core.handle_timeout(h.now);
    h.until(|h| {
        h.core.expires() == Some(UNIX + (Duration::from_secs(540) + life).as_micros() as u64)
    });
    assert_eq!(
        h.core.next_deadline(),
        Some(first + Duration::from_secs(540))
    );
}
