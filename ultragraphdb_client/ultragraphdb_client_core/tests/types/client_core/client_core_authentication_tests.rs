/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_client_core::{
    BatchPoll, ClientCore, ClientErrorEnum, Reply, TEST_NAME, TEST_PASSWORD, TEST_PRINCIPAL,
    TestServer,
};
use ultragraphdb_protocol_scram::{ScramError, ScramLogin};
use ultragraphdb_protocol_transport::{Anonymous, BufferPool, NetworkFaults, TransportError};
use ultragraphdb_protocol_wire::{
    AuthenticateView, BodyBuilder, BodyView, Command, FrameView, Header, HeaderView,
    MAX_FRAME_SIZE, PatternSlot, QuadPattern, Read, TermTable,
};

use crate::utils::harness::{
    CLUSTER, Harness, TestLogin, UNIX, begun, committed, config, login, nonce, received,
    registered, scram_welcome, start,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

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
fn welcome_frame() -> TestResult<Vec<u8>> {
    Ok(scram_welcome().frame(Header::new(CLUSTER, Command::Welcome).with_sequence(1))?)
}

/// A Connections authenticate scenario: a client authenticates with SCRAM-SHA-256.
#[test]
fn test_a_client_authenticates_with_scram_and_its_requests_are_served() -> TestResult {
    let mut h = Harness::authenticated(scram_server())?;
    assert_eq!(h.core.principal(), Some(TEST_PRINCIPAL));
    assert_eq!(h.server.principal(), Some(TEST_PRINCIPAL));
    assert_eq!(h.core.expires(), None, "the credential never expires");
    assert_eq!(
        exchanges(&h),
        vec![0, 2],
        "the client's first and final messages"
    );
    let p = h.core.register(h.now)?;
    assert!(matches!(h.settle(p)?, Ok(Reply::Registered(_))));
    assert_eq!(h.core.next_deadline(), None, "no renewal is due");
    Ok(())
}

#[test]
fn test_requests_wait_for_the_final_authenticate() -> TestResult {
    let pool = BufferPool::new(2, 1 << 20);
    let now = start();
    let mut core = ClientCore::new(config(), login()?);
    core.connect(now, UNIX)?;
    let _hello: Option<Vec<u8>> = core.transmit();
    core.receive(received(&pool, &welcome_frame()?)?, now)?;
    assert!(!core.is_ready(), "authenticating");
    assert_eq!(core.welcome(), Some(&scram_welcome()));
    assert_eq!(
        core.register(now).err().context("an error")?.0,
        ClientErrorEnum::NotConnected
    );
    let first = core.transmit().context("the client's first message")?;
    let header = HeaderView::new(&first)?;
    assert_eq!((header.stream(), header.sequence()), (0, 2));
    let view = FrameView::validate_client(&first, CLUSTER, MAX_FRAME_SIZE)?;
    let message = AuthenticateView::from_frame(&view)?;
    assert_eq!((message.step(), message.mechanism()), (0, "SCRAM-SHA-256"));
    assert_eq!(
        core.next_deadline(),
        Some(now + Duration::from_secs(1)),
        "the exchange is awaited within the configured timeout, 1 s"
    );
    Ok(())
}

#[test]
fn test_a_wrong_password_closes_the_connection() -> TestResult {
    let wrong = ScramLogin::new(
        TEST_NAME,
        "wrong",
        nonce as fn() -> Result<String, ScramError>,
    )?;
    let mut h = Harness::with(config(), scram_server(), NetworkFaults::none(), 1, wrong);
    h.connect()?;
    h.until(|h| !h.core.is_connected())?;
    assert_eq!(h.core.principal(), None);
    assert_eq!(h.server.principal(), None);
    assert!(h.server.is_closing());
    Ok(())
}

#[test]
fn test_a_login_speaking_no_offered_mechanism_gives_the_connection_up() -> TestResult {
    let pool = BufferPool::new(2, 1 << 20);
    let now = start();
    let mut core = ClientCore::new(config(), Anonymous);
    core.connect(now, UNIX)?;
    let _hello: Option<Vec<u8>> = core.transmit();
    let refused = core
        .receive(received(&pool, &welcome_frame()?)?, now)
        .err()
        .context("an error")?;
    assert_eq!(
        refused.0,
        ClientErrorEnum::Transport {
            error: TransportError::authentication(
                "the login speaks none of the server's mechanisms"
            )
        }
    );
    assert!(!core.is_connected());
    Ok(())
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
fn test_a_credential_is_renewed_before_it_expires_while_a_read_streams() -> TestResult {
    let life = Duration::from_secs(600);
    let mut h = Harness::authenticated(scram_server().with_credential_life(life).with_results(6))?;
    let first = UNIX + u64::try_from(life.as_micros())?;
    assert_eq!(h.core.expires(), Some(first));
    let p = h.core.begin(0, true, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let stream = h.core.read(&tx, &read(tx.stream()), h.now)?;
    h.until(|h| h.server.received().len() >= 5)?;
    while h.step()? {}
    let renew = h.core.next_deadline().context("the renewal is due")?;
    assert_eq!(
        renew,
        h.now + Duration::from_secs(540),
        "60 s before the expiry"
    );
    h.now = renew;
    h.core.handle_timeout(h.now);
    let renewed = UNIX + u64::try_from((Duration::from_secs(540) + life).as_micros())?;
    h.until(|h| h.core.expires() == Some(renewed))?;
    assert_eq!(
        exchanges(&h),
        vec![0, 2, 0, 2],
        "a second exchange on stream 0"
    );
    assert_eq!(h.core.principal(), Some(TEST_PRINCIPAL));
    let mut batches = 0;
    let mut failure = None;
    h.until(|h| {
        loop {
            match h.core.next_batch(&stream, h.now, |_| ()) {
                Ok(BatchPoll::Batch(())) => batches += 1,
                Ok(BatchPoll::Done) => return true,
                Ok(BatchPoll::Wait) => return false,
                Err(error) => {
                    failure = Some(error);
                    return true;
                }
            }
        }
    })?;
    if let Some(error) = failure {
        return Err(error).context("a batch, the end or a wait");
    }
    assert_eq!(batches, 6, "the read went on through the renewal");
    assert_eq!(h.connections, 1);
    Ok(())
}

#[test]
fn test_a_short_credential_is_renewed_at_half_its_life() -> TestResult {
    let h = Harness::authenticated(scram_server().with_credential_life(Duration::from_secs(60)))?;
    assert_eq!(
        h.core.next_deadline(),
        Some(h.now + Duration::from_secs(30))
    );
    Ok(())
}

#[test]
fn test_a_new_connection_forgets_the_principal_until_it_authenticates_again() -> TestResult {
    let mut h = Harness::authenticated(scram_server())?;
    h.connect()?;
    assert_eq!(h.core.principal(), None);
    h.until(|h| h.core.is_ready())?;
    assert_eq!(h.core.principal(), Some(TEST_PRINCIPAL));
    assert_eq!(exchanges(&h), vec![0, 2, 0, 2]);
    Ok(())
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
    fn authenticated(expires: u64) -> TestResult<Self> {
        let mut played = Self {
            core: ClientCore::new(config(), Plain),
            pool: BufferPool::new(4, 1 << 20),
            framer: ultragraphdb_protocol_transport::Framer::new(CLUSTER, MAX_FRAME_SIZE),
            sent: 0,
        };
        played.core.connect(start(), UNIX)?;
        let welcome = crate::utils::harness::welcome()
            .with_mechanisms(["TEST"])
            .frame(Header::new(CLUSTER, Command::Welcome))?;
        played.play(&welcome)??;
        played.play(&Self::last(7, expires)?)??;
        assert!(played.core.is_ready());
        Ok(played)
    }

    /// The server's final Authenticate of the exchange, naming `principal`.
    fn last(principal: u64, expires: u64) -> TestResult<Vec<u8>> {
        Ok(
            ultragraphdb_protocol_wire::Authenticate::new(1, "", b"ok".to_vec())
                .with_final(principal, expires)
                .frame(Header::new(CLUSTER, Command::Authenticate))?,
        )
    }

    /// `frame` delivered as the connection's next: the core's answer, or the test's failure when `frame` is no whole
    /// frame.
    fn play(
        &mut self,
        frame: &[u8],
    ) -> TestResult<Result<(), ultragraphdb_client_core::ClientError>> {
        self.sent += 1;
        let mut stamped = frame.to_vec();
        Header::stamp_sequence(&mut stamped, self.sent)?;
        let received =
            crate::utils::harness::frame_through(&mut self.framer, &self.pool, &stamped)?;
        Ok(self.core.receive(received, start()))
    }
}

#[test]
fn test_a_renewal_ending_as_another_principal_gives_the_connection_up() -> TestResult {
    let expires = UNIX + 600_000_000;
    let mut played = Played::authenticated(expires)?;
    // The Hello and the first exchange's step 0, frames 1 and 2, go first.
    assert_eq!(std::iter::from_fn(|| played.core.transmit()).count(), 2);
    let renew = played.core.next_deadline().context("a deadline")?;
    assert_eq!(played.core.transmit(), None, "nothing before the renewal");
    played.core.handle_timeout(renew);
    // The renewal's first message: step 0 on stream 0, naming the mechanism, the connection's frame after the
    // first exchange's step 0.
    let first = played
        .core
        .transmit()
        .context("the renewal's first message")?;
    let header = HeaderView::new(&first)?;
    assert_eq!((header.stream(), header.sequence()), (0, 3));
    let view = FrameView::validate_client(&first, CLUSTER, MAX_FRAME_SIZE)?;
    let message = AuthenticateView::from_frame(&view)?;
    assert_eq!((message.step(), message.mechanism()), (0, "TEST"));
    let refused = played
        .play(&Played::last(8, expires + 600_000_000)?)?
        .err()
        .context("an error")?;
    assert_eq!(
        refused.0,
        ClientErrorEnum::Transport {
            error: TransportError::authentication("renewed as another principal")
        }
    );
    assert!(!played.core.is_connected());
    Ok(())
}

#[test]
fn test_a_server_that_does_not_prove_itself_gives_the_connection_up() -> TestResult {
    let played = Played::authenticated(0)?;
    assert_eq!(played.core.principal(), Some(7));
    assert_eq!(played.core.expires(), None);
    let mut fresh = Played {
        core: ClientCore::new(config(), Plain),
        pool: BufferPool::new(4, 1 << 20),
        framer: ultragraphdb_protocol_transport::Framer::new(CLUSTER, MAX_FRAME_SIZE),
        sent: 0,
    };
    fresh.core.connect(start(), UNIX)?;
    let welcome = crate::utils::harness::welcome()
        .with_mechanisms(["TEST"])
        .frame(Header::new(CLUSTER, Command::Welcome))?;
    fresh.play(&welcome)??;
    let forged = ultragraphdb_protocol_wire::Authenticate::new(1, "", b"no".to_vec())
        .with_final(7, 0)
        .frame(Header::new(CLUSTER, Command::Authenticate))?;
    let refused = fresh.play(&forged)?.err().context("an error")?;

    assert_eq!(
        refused.0,
        ClientErrorEnum::Transport {
            error: TransportError::authentication("the server did not prove itself")
        }
    );
    assert!(!fresh.core.is_connected());
    assert_eq!(fresh.core.principal(), None);
    Ok(())
}

#[test]
fn test_a_commit_lost_with_its_connection_is_retried_once_the_next_one_authenticated() -> TestResult
{
    let mut h = Harness::authenticated(scram_server())?;
    let p = h.core.register(h.now)?;
    let session = registered(h.settle(p)?)?;
    let beginning = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(beginning)?)?;
    h.server.drop_after_next_commit();
    let committing = h.core.commit(tx, session, h.now)?;
    let committed = committed(h.settle(committing)?)?;
    assert_eq!(committed.request(), 1);
    assert_eq!(h.server.applied(), 1);
    assert_eq!(h.connections, 2);
    assert_eq!(
        exchanges(&h),
        vec![0, 2, 0, 2],
        "the retry waited for the second exchange"
    );
    Ok(())
}

/// The second renewal is due its margin before the second expiry, on the wall clock as it has moved.
#[test]
fn test_each_renewal_is_scheduled_from_the_wall_clock_at_its_end() -> TestResult {
    let life = Duration::from_secs(600);
    let mut h = Harness::authenticated(scram_server().with_credential_life(life))?;
    let first = h.core.next_deadline().context("a deadline")?;
    h.now = first;
    h.core.handle_timeout(h.now);
    let renewed = UNIX + u64::try_from((Duration::from_secs(540) + life).as_micros())?;
    h.until(|h| h.core.expires() == Some(renewed))?;
    assert_eq!(
        h.core.next_deadline(),
        Some(first + Duration::from_secs(540))
    );
    Ok(())
}

#[test]
fn test_an_unknown_principal_closes_the_connection() -> TestResult {
    let unknown = ScramLogin::new(
        "bob",
        TEST_PASSWORD,
        nonce as fn() -> Result<String, ScramError>,
    )?;
    let mut h = Harness::with(config(), scram_server(), NetworkFaults::none(), 1, unknown);
    h.connect()?;
    h.until(|h| !h.core.is_connected())?;
    assert_eq!(h.core.principal(), None);
    assert_eq!(h.server.principal(), None);
    assert!(h.server.is_closing());
    Ok(())
}

/// A login starting a mechanism the Welcome did not list is refused by the server, which closes the connection.
#[test]
fn test_a_mechanism_the_server_does_not_speak_closes_the_connection() -> TestResult {
    let mut h = Harness::with(config(), scram_server(), NetworkFaults::none(), 1, Plain);
    h.connect()?;
    h.until(|h| !h.core.is_connected())?;
    assert_eq!(h.core.principal(), None);
    assert_eq!(h.server.principal(), None);
    assert!(h.server.is_closing());
    Ok(())
}

/// A login offering [`Plain`] once: asked again, for a renewal, it speaks none of the server's mechanisms.
#[derive(Debug, Default)]
struct Once {
    used: bool,
}

impl ultragraphdb_protocol_transport::Login for Once {
    type Mechanism = Plain;

    fn mechanism(
        &mut self,
        _offered: &[String],
    ) -> Result<Option<Plain>, ultragraphdb_protocol_transport::TransportError> {
        Ok((!std::mem::replace(&mut self.used, true)).then_some(Plain))
    }
}

/// A renewal the login cannot start gives the connection up, as a first exchange it cannot start does.
#[test]
fn test_a_renewal_the_login_cannot_start_gives_the_connection_up() -> TestResult {
    let pool = BufferPool::new(4, 1 << 20);
    let mut framer = ultragraphdb_protocol_transport::Framer::new(CLUSTER, MAX_FRAME_SIZE);
    let mut core = ClientCore::new(config(), Once::default());
    let now = start();
    core.connect(now, UNIX)?;
    let welcome = crate::utils::harness::welcome()
        .with_mechanisms(["TEST"])
        .frame(Header::new(CLUSTER, Command::Welcome))?;
    let expires = UNIX + 600_000_000;
    let last = Played::last(7, expires)?;
    for (sequence, frame) in [(1u128, welcome), (2, last)] {
        let mut stamped = frame;
        Header::stamp_sequence(&mut stamped, sequence)?;
        let received = crate::utils::harness::frame_through(&mut framer, &pool, &stamped)?;
        core.receive(received, now)?;
    }
    assert!(core.is_ready());
    let renew = core.next_deadline().context("the renewal")?;
    // A request waiting when the renewal is due, well within its timeout.
    let p = core.register(renew)?;
    core.handle_timeout(renew);
    assert!(!core.is_connected(), "given up");
    // The request's connection was lost, the login's refusal its cause, as for every other loss.
    assert_eq!(
        core.take_reply(p).context("an answer")?,
        Err(ultragraphdb_client_core::ClientError::connection_lost(
            Some(ultragraphdb_client_core::ClientError::transport(
                TransportError::authentication("the login speaks none of the server's mechanisms")
            ))
        ))
    );
    Ok(())
}

/// While a renewal runs, the core waits for the server's answer to it: its next deadline is that wait's timeout, the
/// renewal's start plus the configured timeout, not the renewal's own time, which has passed.
#[test]
fn test_a_renewal_under_way_is_due_at_its_timeout() -> TestResult {
    let mut played = Played::authenticated(UNIX + 600_000_000)?;
    let renew = played.core.next_deadline().context("the renewal")?;
    played.core.handle_timeout(renew);
    assert!(
        played.core.transmit().is_some(),
        "the renewal's first message"
    );
    assert_eq!(
        played.core.next_deadline(),
        Some(renew + config().timeout())
    );
    Ok(())
}

/// A login under the name `SIZED` whose first message is as many bytes long as it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sized(usize);

impl ultragraphdb_protocol_transport::ClientMechanism for Sized {
    type Error = &'static str;

    fn name(&self) -> &str {
        "SIZED"
    }

    fn start(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(vec![b'x'; self.0])
    }

    fn answer(&mut self, _challenge: &[u8]) -> Result<Vec<u8>, &'static str> {
        Ok(Vec::new())
    }

    fn finish(&mut self, _outcome: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }
}

impl ultragraphdb_protocol_transport::Login for Sized {
    type Mechanism = Sized;

    fn mechanism(
        &mut self,
        _offered: &[String],
    ) -> Result<Option<Sized>, ultragraphdb_protocol_transport::TransportError> {
        Ok(Some(*self))
    }
}

/// The core's answer to a Welcome, and the frame it queued after its Hello.
type Welcomed = (
    Result<(), ultragraphdb_client_core::ClientError>,
    Option<Vec<u8>>,
);

/// The core of the [`Sized`] login of `message` bytes, welcomed by a server allowing frames up to `max_frame`: its
/// answer to the Welcome, and the frame it queued after its Hello.
fn sized_exchange(message: usize, max_frame: u32) -> TestResult<Welcomed> {
    let mut core = ClientCore::new(config(), Sized(message));
    core.connect(start(), UNIX)?;
    let _hello = core.transmit().context("the Hello")?;
    let welcome = crate::utils::harness::welcome()
        .with_max_frame(max_frame)
        .with_mechanisms(["SIZED"])
        .frame(Header::new(CLUSTER, Command::Welcome).with_sequence(1))?;
    let pool = BufferPool::new(2, 1 << 20);
    let answer = core.receive(received(&pool, &welcome)?, start());
    Ok((answer, core.transmit()))
}

/// An Authenticate exactly as large as the Welcome's `max_frame` is sent; one byte larger is refused before it is sent
/// (`PROTOCOL.md` §2.5: a frame up to `max_frame` bytes).
#[test]
fn test_an_authenticate_of_the_welcomes_largest_frame_is_sent() -> TestResult {
    const MAX: u32 = 524_288;
    let limit = MAX as usize;
    let size = |message: usize| -> TestResult<usize> {
        Ok(
            ultragraphdb_protocol_wire::Authenticate::new(0, "SIZED", vec![b'x'; message])
                .frame(Header::new(CLUSTER, Command::Authenticate))?
                .len(),
        )
    };
    // A frame is its body and a chunk table growing with the body (`PROTOCOL.md` §2.3), so the message is sized again
    // until the frame comes out at the limit.
    let mut message = limit - size(0)?;
    for _ in 0..8 {
        let got = size(message)?;
        if got == limit {
            break;
        }
        message = (message + limit)
            .checked_sub(got)
            .context("a shorter message")?;
    }
    assert_eq!(
        size(message)?,
        limit,
        "the message fills the frame to its limit"
    );
    let (answer, sent) = sized_exchange(message, MAX)?;
    answer?;
    assert_eq!(sent.context("the first Authenticate")?.len(), limit);
    let (refused, none) = sized_exchange(message + 1, MAX)?;
    assert!(
        matches!(refused.err().context("a refusal")?.0, ClientErrorEnum::Request { error: ref e } if matches!(e.0, ultragraphdb_error_protocol::ProtocolErrorEnum::FrameTooLarge { limit: 524_288, .. })),
        "one byte past the limit"
    );
    assert_eq!(none, None, "nothing is sent");
    Ok(())
}
