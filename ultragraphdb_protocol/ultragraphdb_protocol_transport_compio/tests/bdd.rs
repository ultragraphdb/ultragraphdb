/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conversations of the conformance suite over the transport: a client and a server connection on the simulated
//! network, the server's connection run by `ServerConnection` and authenticated by SCRAM-SHA-256, its frames scheduled
//! and numbered by its `Outbound`; the client's frames numbered here, as a client does.

use std::time::{Duration, Instant};

use cucumber::{World, then, when};
use ultragraphdb_protocol_scram::{Credentials, ScramAuthenticator, ScramClient, ScramVerifier};
use ultragraphdb_protocol_transport::{
    AUTHENTICATION_TIMEOUT, AuthenticationStep, Authenticator, BufferPool, ClientAuthentication,
    ClientHandshake, Credential, Framer, Inbound, MechanismStep, NetworkFaults, ServerConnection,
    ServerMechanism, SimNetwork, TransportErrorEnum,
};
use ultragraphdb_protocol_transport_compio::{CompioSimStream, Connection};
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, BodyView, Cancel, Command, Commit, Credit, ErrorCode, ErrorReply,
    ErrorReplyView, FLAG_LAST, FrameView, Header, Hello, MAX_FRAME_SIZE, PatternSlot, QuadPattern,
    Read, Register, RequestTerm, ResultBatch, TermTable, Welcome, Write, WriteView,
};

// The bounded executor of the unit tests, shared by path.
mod utils {
    #[path = "ready.rs"]
    pub mod ready;
    #[path = "sim.rs"]
    pub mod sim;
}

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::ready::ready;
use crate::utils::sim::connect;

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
const SCRAM: &str = "SCRAM-SHA-256";
/// The expiry a renewed credential states on the wire, microseconds of Unix time.
const EXPIRES: u64 = 1_798_761_600_000_000;

/// A frame the client received: its stream, command and flags.
type Seen = (u64, u16, u8);

#[derive(Debug, Default, World)]
struct Steps {
    seen: Vec<Seen>,
    later: Vec<Seen>,
    queued: usize,
    welcome: Option<Welcome>,
    welcomed: Option<Welcome>,
    inbound: Vec<Inbound>,
    errors: Vec<(u64, ErrorCode, Vec<u8>)>,
    principal: Option<u64>,
    credits: Vec<Option<u32>>,
}

/// The principals a server holds: alice, 7, and bob, 8, both with password `pw`, derived at the least count so the
/// conversations run fast.
#[derive(Debug, Clone)]
struct Users {
    alice: ScramVerifier,
    bob: ScramVerifier,
}

impl Users {
    /// Both principals' verifiers, each salted with its name.
    fn new() -> TestResult<Self> {
        Ok(Self {
            alice: ScramVerifier::derive("pw", b"alice", 4_096)?,
            bob: ScramVerifier::derive("pw", b"bob", 4_096)?,
        })
    }
}

impl Credentials for Users {
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        match name {
            "alice" => Some((7, self.alice.clone())),
            "bob" => Some((8, self.bob.clone())),
            _ => None,
        }
    }
}

/// The server's authenticator: SCRAM-SHA-256, every credential lasting `lifetime` when one is given.
#[derive(Debug, Clone)]
struct Server {
    scram: ScramAuthenticator<Users, fn() -> String>,
    lifetime: Option<Duration>,
}

fn nonce() -> String {
    "server-nonce".to_string()
}

impl Server {
    fn new(lifetime: Option<Duration>) -> TestResult<Self> {
        Ok(Self {
            scram: ScramAuthenticator::new(Users::new()?, nonce as fn() -> String, [3; 32])
                .with_iterations(4_096),
            lifetime,
        })
    }
}

struct Exchange<M> {
    inner: M,
    lifetime: Option<Duration>,
}

impl<M: ServerMechanism> ServerMechanism for Exchange<M> {
    type Error = M::Error;

    fn step(&mut self, message: &[u8]) -> Result<MechanismStep, M::Error> {
        Ok(match (self.inner.step(message)?, self.lifetime) {
            (MechanismStep::Done(message, credential), Some(lifetime)) => MechanismStep::Done(
                message,
                Credential::expiring(credential.principal(), EXPIRES, lifetime),
            ),
            (step, _) => step,
        })
    }
}

impl Authenticator for Server {
    type Mechanism =
        Exchange<<ScramAuthenticator<Users, fn() -> String> as Authenticator>::Mechanism>;

    fn start(&mut self, mechanism: &str) -> Option<Self::Mechanism> {
        Some(Exchange {
            inner: self.scram.start(mechanism)?,
            lifetime: self.lifetime,
        })
    }
}

/// A point in time to start from. The connection never reads a clock; the conversations move this value on
/// themselves, so the clock is read once, here, for an origin only.
#[allow(clippy::disallowed_methods)]
fn start() -> Instant {
    Instant::now()
}

/// Both ends of a connection: the client's, which numbers its frames, and the server's, run by its connection machine.
struct Conversation {
    network: SimNetwork,
    client: Connection<CompioSimStream>,
    server: Connection<CompioSimStream>,
    probe: CompioSimStream,
    connection: ServerConnection<Server>,
    pool: BufferPool,
    now: Instant,
    sent: u128,
}

impl Conversation {
    /// A client connected to a server welcoming as `welcome` does, past the Hello, its Welcome read.
    async fn new(
        faults: NetworkFaults,
        welcome: Welcome,
        lifetime: Option<Duration>,
    ) -> TestResult<Self> {
        let network = SimNetwork::new(17, faults);
        let (client, server) = connect(&network);
        let probe = client.clone();
        let now = start();
        let mut c = Self {
            network,
            client: Connection::new(client, Framer::new(CLUSTER, MAX_FRAME_SIZE)),
            server: Connection::new(
                server,
                Framer::from_client(CLUSTER, welcome.max_frame() as usize),
            ),
            probe,
            connection: ServerConnection::new(CLUSTER, welcome, Server::new(lifetime)?, now),
            pool: BufferPool::new(64, MAX_FRAME_SIZE),
            now,
            sent: 0,
        };
        let hello = ClientHandshake::new(
            CLUSTER,
            Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "client"),
        );
        c.client_sends(hello.hello_frame()?).await?;
        assert!(matches!(c.server_reads().await?, Inbound::Welcomed { .. }));
        c.server_sends(usize::MAX).await?;
        c.network.run_until_idle();
        let answer = ready(c.client.read_frame(&c.pool))?;
        let welcomed = hello
            .receive(answer.bytes())
            .context("the client is welcomed")?;
        c.client.framer_mut().set_protocol(welcomed.protocol());
        Ok(c)
    }

    /// The client sends `frame`, stamped with its next sequence.
    async fn client_sends(&mut self, mut frame: Vec<u8>) -> TestResult {
        self.sent += 1;
        Header::stamp_sequence(&mut frame, self.sent)?;
        let _sent: Vec<u8> = ready(self.client.write_frame(frame))?;
        Ok(())
    }

    /// The client sends `body` on `stream`.
    async fn send<B: BodyBuilder>(&mut self, stream: u64, body: &B) -> TestResult {
        let frame = body.frame(Header::new(CLUSTER, Command::Abort).with_stream(stream))?;
        self.client_sends(frame).await
    }

    /// The server reads one frame and its connection takes it; as its session, the server refuses a Write whose body
    /// breaks a rule. A frame the framer refuses closes the connection with code 1.
    async fn server_reads(&mut self) -> TestResult<Inbound> {
        self.network.run_until_idle();
        let received = match ready(self.server.read_frame(&self.pool)) {
            Ok(received) => received,
            Err(refusal) => {
                let protocol = if let TransportErrorEnum::Protocol { error } = &refusal.0 {
                    Some(error)
                } else {
                    None
                };
                let error = protocol
                    .ok_or_else(|| format!("{refusal:?}"))
                    .context("the framer refuses by a protocol rule")?;
                return Ok(self.connection.refuse_connection(error));
            }
        };
        let inbound = self.connection.receive(&received, self.now);
        if let Inbound::Welcomed { protocol } = inbound {
            self.server.framer_mut().set_protocol(protocol);
        }
        if let Inbound::Request {
            stream,
            command: Command::Write,
        } = inbound
        {
            let view = received.view()?;
            if let Err(rule) = WriteView::from_frame(&view) {
                self.connection.refuse(stream, &ErrorReply::protocol(&rule));
            }
        }
        Ok(inbound)
    }

    /// The server reads one frame and its connection takes it, as [`Conversation::server_reads`] does, where the
    /// conversation does not look at what the connection made of it.
    async fn server_takes(&mut self) -> TestResult {
        let _taken: Inbound = self.server_reads().await?;
        Ok(())
    }

    /// The server sends up to `most` frames its connection lets go.
    async fn server_sends(&mut self, most: usize) -> TestResult {
        for _ in 0..most {
            match self.connection.next_frame() {
                Some((_, frame)) => {
                    let _sent: Vec<u8> = ready(self.server.write_frame(frame))?;
                }
                None => break,
            }
        }
        Ok(())
    }

    /// Every frame that has reached the client, whole.
    async fn client_frames(&mut self) -> TestResult<Vec<Vec<u8>>> {
        self.network.run_until_idle();
        let mut frames = Vec::new();
        while self.probe.sim().pending() > 0 {
            let frame = ready(self.client.read_frame(&self.pool))?;
            let _validated: FrameView<'_> = frame
                .view()
                .context("every server frame validates, numbered in order")?;
            frames.push(frame.bytes().to_vec());
        }
        Ok(frames)
    }

    /// Every frame that has reached the client: its stream, command and flags.
    async fn client_reads(&mut self) -> TestResult<Vec<Seen>> {
        self.client_frames()
            .await?
            .iter()
            .map(|f| seen(f))
            .collect()
    }

    /// The server queues a result of `frames` frames on `stream`, the last flagged.
    fn result(&mut self, stream: u64, frames: u32) -> TestResult {
        for i in 0..frames {
            let last = i + 1 == frames;
            self.connection
                .outbound()
                .push(stream, result(stream, last)?)?;
        }
        Ok(())
    }

    /// `name`'s exchange with `password`: the principal it authenticated, or the server's refusal and its frame.
    async fn authenticate(
        &mut self,
        name: &str,
        password: &str,
    ) -> TestResult<Result<u64, (ErrorCode, Vec<u8>)>> {
        let mut auth = ClientAuthentication::new(ScramClient::new(name, password, "client-nonce")?);
        let mut message = auth.start()?;
        loop {
            self.send(0, &message).await?;
            self.server_takes().await?;
            self.server_sends(usize::MAX).await?;
            let frames = self.client_frames().await?;
            let mut answers = Vec::new();
            for frame in frames {
                if seen(&frame)?.0 == 0 {
                    answers.push(frame);
                }
            }
            let answer = answers
                .into_iter()
                .next()
                .context("the server's answer on stream 0")?;
            let view = FrameView::validate(&answer, CLUSTER, MAX_FRAME_SIZE)?;
            match auth.receive(&view) {
                Ok(AuthenticationStep::Send(next)) => message = next,
                Ok(AuthenticationStep::Done { principal, .. }) => return Ok(Ok(principal)),
                Err(_) => {
                    let error = ErrorReplyView::from_frame(&view)?;
                    return Ok(Err((error.code(), answer)));
                }
            }
        }
    }
}

fn seen(frame: &[u8]) -> TestResult<Seen> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    let header = view.header();
    Ok((header.stream(), header.command(), header.flags()))
}

fn code(frame: &[u8]) -> TestResult<ErrorCode> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    Ok(ErrorReplyView::from_frame(&view)?.code())
}

/// Each frame's stream, its error code and the frame, every frame an Error frame.
fn errors(frames: &[Vec<u8>]) -> TestResult<Vec<(u64, ErrorCode, Vec<u8>)>> {
    frames
        .iter()
        .map(|f| Ok((seen(f)?.0, code(f)?, f.clone())))
        .collect()
}

fn result(stream: u64, last: bool) -> TestResult<Vec<u8>> {
    Ok(ResultBatch::new(1).frame(
        Header::new(CLUSTER, Command::Result)
            .with_stream(stream)
            .with_flags(if last { FLAG_LAST } else { 0 }),
    )?)
}

fn welcome(initial_credit: u32) -> Welcome {
    Welcome::new(1, 0, "server")
        .with_max_frame(1 << 20)
        .with_max_streams(16)
        .with_initial_credit(initial_credit)
}

fn authenticating(initial_credit: u32) -> Welcome {
    welcome(initial_credit).with_mechanisms([SCRAM])
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

fn commit_reply(stream: u64) -> TestResult<Vec<u8>> {
    Ok(Commit::new(0, 0, 9, Vec::new()).frame(
        Header::new(CLUSTER, Command::Commit)
            .with_stream(stream)
            .with_flags(FLAG_LAST),
    )?)
}

/// A conversation authenticated as alice, its credential lasting `lifetime`.
async fn serving(initial_credit: u32, lifetime: Option<Duration>) -> TestResult<Conversation> {
    let mut c = Conversation::new(
        NetworkFaults::none(),
        authenticating(initial_credit),
        lifetime,
    )
    .await?;
    assert_eq!(c.authenticate("alice", "pw").await?, Ok(7));
    Ok(c)
}

// Handshake, streams, credit and cancellation.

async fn credit_conversation(w: &mut Steps, faults: NetworkFaults) -> TestResult {
    let mut c = Conversation::new(faults, welcome(0), None).await?;
    c.connection.outbound().open(1)?;
    c.result(1, 10)?;
    c.server_sends(usize::MAX).await?;
    assert!(c.client_reads().await?.is_empty(), "no credit, no frame");
    c.send(1, &Credit::new(2)).await?;
    assert_eq!(c.server_reads().await?, Inbound::Handled);
    c.server_sends(usize::MAX).await?;
    w.seen = c.client_reads().await?;
    w.queued = c.connection.outbound().queued(1);
    c.send(1, &Credit::new(8)).await?;
    c.server_takes().await?;
    c.server_sends(usize::MAX).await?;
    w.later = c.client_reads().await?;
    Ok(())
}

#[when("a client grants credit for two frames of a result of ten")]
async fn credit_two(w: &mut Steps) -> TestResult {
    credit_conversation(w, NetworkFaults::none()).await?;
    Ok(())
}

#[when(
    "a client grants credit for two frames of a result of ten over a network that splits and delays frames"
)]
async fn credit_two_faulty(w: &mut Steps) -> TestResult {
    credit_conversation(
        w,
        NetworkFaults::none()
            .with_split(700_000)
            .with_delay(500_000, 6),
    )
    .await?;
    Ok(())
}

#[then("the server sends two frames and waits until more credit arrives")]
fn two_then_wait(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen, vec![(1, 13, 0), (1, 13, 0)]);
    assert_eq!(w.queued, 8, "the other eight wait for credit");
    assert_eq!(w.later.len(), 8);
    assert!(w.later[..7].iter().all(|s| *s == (1, 13, 0)));
    assert_eq!(w.later[7], (1, 13, FLAG_LAST));
    Ok(())
}

#[when("a client cancels a stream mid-result")]
async fn cancel_mid_result(w: &mut Steps) -> TestResult {
    let mut c = Conversation::new(NetworkFaults::none(), welcome(10), None).await?;
    c.connection.outbound().open(1)?;
    c.connection.outbound().open(2)?;
    c.result(1, 5)?;
    c.result(2, 5)?;
    c.server_sends(2).await?;
    w.seen = c.client_reads().await?;
    c.send(1, &Cancel).await?;
    assert_eq!(
        c.server_reads().await?,
        Inbound::Cancelled {
            stream: 1,
            transaction: false
        }
    );
    c.server_sends(usize::MAX).await?;
    w.later = c.client_reads().await?;
    Ok(())
}

#[then("no further frame of that stream arrives, and other streams continue")]
fn cancelled(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen, vec![(1, 13, 0), (2, 13, 0)]);
    assert!(
        w.later.iter().all(|(stream, _, _)| *stream == 2),
        "{:?}",
        w.later
    );
    assert_eq!(w.later.len(), 4);
    assert_eq!(w.later[3].2, FLAG_LAST);
    Ok(())
}

#[when("a client speaking protocols 1 to 3 says Hello to a server speaking 1 to 2")]
async fn hello_one_to_three(w: &mut Steps) -> TestResult {
    let network = SimNetwork::new(5, NetworkFaults::none());
    let (client, server) = connect(&network);
    let mut client = Connection::new(client, Framer::new(CLUSTER, MAX_FRAME_SIZE));
    let mut server = Connection::new(server, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let limits = Welcome::new(2, 0x0005_0000, "server")
        .with_max_frame(1 << 20)
        .with_max_streams(32)
        .with_initial_credit(4);
    let (hello, answered) = ready(futures::future::join(
        client.hello(
            &pool,
            &Hello::new(1, 3, 0x0005_0000, MAX_FRAME_SIZE as u32, "client"),
        ),
        server.welcome(&pool, &limits),
    ));
    w.welcome = Some(hello?);
    w.welcomed = Some(answered?);
    Ok(())
}

#[then("the server welcomes it at protocol 2 with its limits")]
fn welcomed_at_two(w: &mut Steps) -> TestResult {
    let welcome = w.welcome.clone().context("the client's Welcome")?;
    assert_eq!(welcome.protocol(), 2);
    assert_eq!(
        (
            welcome.max_frame(),
            welcome.max_streams(),
            welcome.initial_credit()
        ),
        (1 << 20, 32, 4)
    );
    assert_eq!(w.welcomed, Some(welcome));
    Ok(())
}

#[when("a client sends requests on streams 1 and 2 and the result of stream 2 is ready first")]
async fn two_streams(w: &mut Steps) -> TestResult {
    let mut c =
        Conversation::new(NetworkFaults::none().with_split(500_000), welcome(1), None).await?;
    c.send(1, &Register::new(0)).await?;
    c.send(2, &Register::new(0)).await?;
    c.server_takes().await?;
    c.server_takes().await?;
    let reply = |stream| {
        Commit::new(1, stream, 10 + stream, Vec::new()).frame(
            Header::new(CLUSTER, Command::Commit)
                .with_stream(stream)
                .with_flags(FLAG_LAST),
        )
    };
    // Stream 2's reply is ready first: the server queues it, sends what it may, then stream 1's.
    c.connection.outbound().push(2, reply(2)?)?;
    c.server_sends(usize::MAX).await?;
    c.connection.outbound().push(1, reply(1)?)?;
    c.server_sends(usize::MAX).await?;
    w.seen = c.client_reads().await?;
    Ok(())
}

#[then("the reply of stream 2 arrives before the reply of stream 1, each on its own stream")]
fn any_order(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen, vec![(2, 5, FLAG_LAST), (1, 5, FLAG_LAST)]);
    Ok(())
}

// Frames are validated before use and never panic.

#[when(
    "a Write whose term table repeats an entry arrives on a transaction's stream while another stream reads"
)]
async fn malformed_write(w: &mut Steps) -> TestResult {
    let mut c = serving(4, None).await?;
    c.send(1, &Begin::new(0, 0, false)).await?;
    c.server_takes().await?;
    c.send(3, &read(1)).await?;
    c.server_takes().await?;
    c.connection.outbound().open(5)?;
    c.result(5, 3)?;
    // A Write whose term table holds one IRI twice: two records, each of the same text.
    let mut terms = TermTable::new();
    let _entry: u32 = terms.intern(RequestTerm::Iri("http://ex.org/a".into()))?;
    let write =
        Write::new(terms, Vec::new()).frame(Header::new(CLUSTER, Command::Write).with_stream(1))?;
    let repeated = repeat_term(&write)?;
    c.client_sends(repeated).await?;
    w.inbound.push(c.server_reads().await?);
    c.server_sends(usize::MAX).await?;
    let frames = c.client_frames().await?;
    w.seen = frames.iter().map(|f| seen(f)).collect::<TestResult<_>>()?;
    let error_frames: Vec<Vec<u8>> = frames
        .into_iter()
        .zip(&w.seen)
        .filter(|(_, s)| s.1 == Command::Error.number())
        .map(|(f, _)| f)
        .collect();
    w.errors = errors(&error_frames)?;
    w.principal = c.connection.principal();
    w.queued = usize::from(c.connection.outbound().is_open(3));
    assert!(!c.connection.is_closing());
    Ok(())
}

/// `write`'s term table with its one record repeated, the body laid out again and the frame resealed: a Write the
/// table's one-encoding rule refuses.
fn repeat_term(write: &[u8]) -> TestResult<Vec<u8>> {
    let view = FrameView::validate(write, CLUSTER, MAX_FRAME_SIZE)?;
    let body = view.body();
    let section = |i: usize| -> TestResult<Vec<u8>> {
        let at = 8 * i;
        let offset = u32::from_le_bytes(body[at..at + 4].try_into()?) as usize;
        let len = u32::from_le_bytes(body[at + 4..at + 8].try_into()?) as usize;
        Ok(body[offset..offset + len].to_vec())
    };
    let mut sections: Vec<Vec<u8>> = (0..6).map(section).collect::<TestResult<_>>()?;
    // The second record's texts follow the first's: its first text at the first's end, its second after its first.
    let mut second = sections[2].clone();
    let end = u32::from_le_bytes(second[12..16].try_into()?);
    second[8..12].copy_from_slice(&end.to_le_bytes());
    second[16..20].copy_from_slice(&(2 * end).to_le_bytes());
    sections[2].extend_from_slice(&second);
    let text = sections[3].clone();
    sections[3].extend_from_slice(&text);
    let mut at = 8 * sections.len();
    let mut table = Vec::new();
    let mut data = Vec::new();
    for s in &sections {
        at = at.div_ceil(8) * 8;
        table.extend_from_slice(&(at as u32).to_le_bytes());
        table.extend_from_slice(&(s.len() as u32).to_le_bytes());
        at += s.len();
    }
    data.extend_from_slice(&table);
    for s in &sections {
        data.resize(data.len().div_ceil(8) * 8, 0);
        data.extend_from_slice(s);
    }
    Ok(Header::new(CLUSTER, Command::Write)
        .with_stream(1)
        .encode_frame(&data)?)
}

#[then(
    "the transaction's stream receives an error of code 1 naming the rule, its transaction ends, and the connection and the other stream continue"
)]
fn malformed_answered(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.inbound[0],
        Inbound::Request {
            stream: 1,
            command: Command::Write
        }
    );
    assert_eq!(w.errors.len(), 1);
    let (stream, code, frame) = &w.errors[0];
    assert_eq!((*stream, *code), (1, ErrorCode::Protocol));
    assert_eq!(seen(frame)?.2, FLAG_LAST);
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    assert!(
        ErrorReplyView::from_frame(&view)?
            .message()
            .contains("repeats entry 0"),
        "the message names the rule"
    );
    assert_eq!(w.queued, 0, "the transaction's read ended with it");
    assert_eq!(
        w.seen.iter().filter(|s| s.0 == 5).count(),
        3,
        "the other stream continued"
    );
    assert_eq!(w.principal, Some(7));
    Ok(())
}

#[when("a client sends Begin on stream 0")]
async fn begin_on_zero(w: &mut Steps) -> TestResult {
    let mut c = serving(4, None).await?;
    c.send(0, &Begin::new(0, 0, false)).await?;
    w.inbound.push(c.server_reads().await?);
    c.server_sends(usize::MAX).await?;
    w.errors = errors(&c.client_frames().await?)?;
    w.principal = c.connection.is_closing().then_some(0);
    Ok(())
}

#[then("the server answers an error of code 1 on stream 0 and closes the connection")]
fn protocol_on_zero(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.inbound,
        vec![Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }]
    );
    assert_eq!(
        w.errors.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        vec![(0, ErrorCode::Protocol)]
    );
    assert_eq!(w.principal, Some(0), "the connection closes");
    Ok(())
}

// Frames are at most 1 MiB.

#[when("a client sends a frame larger than the Welcome's `max_frame`")]
async fn past_max_frame(w: &mut Steps) -> TestResult {
    let small = authenticating(4).with_max_frame(524_288);
    let mut c = Conversation::new(NetworkFaults::none(), small, None).await?;
    let big = ErrorReply::new(ErrorCode::Refused, "x".repeat(600_000));
    c.send(1, &big).await?;
    w.inbound.push(c.server_reads().await?);
    c.server_sends(usize::MAX).await?;
    w.errors = errors(&c.client_frames().await?)?;
    w.principal = c.connection.is_closing().then_some(0);
    Ok(())
}

#[then("the server refuses it as too large, answers on stream 0 and closes the connection")]
fn too_large(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.inbound,
        vec![Inbound::Closing {
            code: Some(ErrorCode::Protocol)
        }]
    );
    let (stream, code, frame) = &w.errors[0];
    assert_eq!((*stream, *code), (0, ErrorCode::Protocol));
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    assert!(
        ErrorReplyView::from_frame(&view)?
            .message()
            .contains("524288-byte limit"),
        "the rule names the limit"
    );
    assert_eq!(w.principal, Some(0));
    Ok(())
}

// Handshake, streams, credit and cancellation: a transaction's reads.

#[when("a transaction runs two reads at once, each on a stream of its own naming the transaction")]
async fn two_reads(w: &mut Steps) -> TestResult {
    let mut c = serving(2, None).await?;
    c.send(1, &Begin::new(0, 0, false)).await?;
    c.server_takes().await?;
    c.send(3, &read(1)).await?;
    c.send(5, &read(1)).await?;
    w.inbound.push(c.server_reads().await?);
    w.inbound.push(c.server_reads().await?);
    w.credits = vec![
        c.connection.outbound().credit(3),
        c.connection.outbound().credit(5),
    ];
    // As the session, at the transaction's snapshot: three frames each, the reads interleaved.
    c.result(3, 3)?;
    c.result(5, 3)?;
    c.server_sends(usize::MAX).await?;
    w.seen = c.client_reads().await?;
    c.send(3, &Credit::new(1)).await?;
    c.send(5, &Credit::new(1)).await?;
    c.server_takes().await?;
    c.server_takes().await?;
    c.server_sends(usize::MAX).await?;
    w.later = c.client_reads().await?;
    c.send(1, &Commit::new(0, 0, 0, Vec::new())).await?;
    w.inbound.push(c.server_reads().await?);
    c.connection.outbound().push(1, commit_reply(1)?)?;
    c.server_sends(usize::MAX).await?;
    w.later.extend(c.client_reads().await?);
    Ok(())
}

#[then(
    "both stream their results at the transaction's snapshot, each with its own credit, and the transaction then commits on its own stream"
)]
fn reads_streamed(w: &mut Steps) -> TestResult {
    assert_eq!(
        &w.inbound[..2],
        &[
            Inbound::Request {
                stream: 3,
                command: Command::Read
            },
            Inbound::Request {
                stream: 5,
                command: Command::Read
            }
        ]
    );
    assert_eq!(
        w.credits,
        vec![Some(2), Some(2)],
        "each read starts with the initial credit"
    );
    // Two frames each on the initial credit, then the last of each on the credit granted.
    assert_eq!(w.seen.iter().filter(|s| s.0 == 3).count(), 2);
    assert_eq!(w.seen.iter().filter(|s| s.0 == 5).count(), 2);
    assert!(w.later.contains(&(3, 13, FLAG_LAST)));
    assert!(w.later.contains(&(5, 13, FLAG_LAST)));
    assert_eq!(
        w.inbound[2],
        Inbound::Request {
            stream: 1,
            command: Command::Commit
        }
    );
    assert!(
        w.later.contains(&(1, 5, FLAG_LAST)),
        "the commit's reply on the transaction's stream"
    );
    Ok(())
}

#[when(
    "a client cancels one of its transaction's reads mid-result and then commits the transaction"
)]
async fn cancel_a_read(w: &mut Steps) -> TestResult {
    let mut c = serving(1, None).await?;
    c.send(1, &Begin::new(0, 0, false)).await?;
    c.server_takes().await?;
    c.send(3, &read(1)).await?;
    c.server_takes().await?;
    c.result(3, 4)?;
    c.server_sends(usize::MAX).await?;
    w.seen = c.client_reads().await?;
    c.send(3, &Cancel).await?;
    w.inbound.push(c.server_reads().await?);
    c.send(1, &Commit::new(0, 0, 0, Vec::new())).await?;
    w.inbound.push(c.server_reads().await?);
    c.connection.outbound().push(1, commit_reply(1)?)?;
    c.server_sends(usize::MAX).await?;
    w.later = c.client_reads().await?;
    Ok(())
}

#[then("no further frame of the read arrives, and the commit succeeds")]
fn read_cancelled(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen, vec![(3, 13, 0)]);
    assert_eq!(
        w.inbound,
        vec![
            Inbound::Cancelled {
                stream: 3,
                transaction: false
            },
            Inbound::Request {
                stream: 1,
                command: Command::Commit
            }
        ]
    );
    assert_eq!(w.later, vec![(1, 5, FLAG_LAST)]);
    Ok(())
}

#[when("a transaction commits while one of its reads has frames left to send")]
async fn commit_ends_reads(w: &mut Steps) -> TestResult {
    let mut c = serving(1, None).await?;
    c.send(1, &Begin::new(0, 0, false)).await?;
    c.server_takes().await?;
    c.send(3, &read(1)).await?;
    c.server_takes().await?;
    c.result(3, 4)?;
    c.server_sends(usize::MAX).await?;
    w.seen = c.client_reads().await?;
    c.send(3, &Credit::new(10)).await?;
    c.send(1, &Commit::new(0, 0, 0, Vec::new())).await?;
    c.server_takes().await?;
    c.server_takes().await?;
    // The commit's reply leaves before the read's next turn; the read ends with its transaction.
    c.connection.outbound().push(1, commit_reply(1)?)?;
    c.server_sends(1).await?;
    c.server_sends(usize::MAX).await?;
    w.later = c.client_reads().await?;
    w.queued = usize::from(c.connection.outbound().is_open(3));
    Ok(())
}

#[then("the read's stream sends no further frame, and the commit's reply arrives")]
fn reads_ended(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen, vec![(3, 13, 0)]);
    assert_eq!(w.later, vec![(1, 5, FLAG_LAST)]);
    assert_eq!(w.queued, 0);
    Ok(())
}

// Connections authenticate.

#[when(
    "a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the principal's password"
)]
async fn scram_success(w: &mut Steps) -> TestResult {
    let mut c = Conversation::new(NetworkFaults::none(), authenticating(4), None).await?;
    w.principal = c.authenticate("alice", "pw").await?.ok();
    c.send(1, &Register::new(0)).await?;
    w.inbound.push(c.server_reads().await?);
    Ok(())
}

#[then("the server's final Authenticate names the principal, and the client's requests are served")]
fn scram_served(w: &mut Steps) -> TestResult {
    assert_eq!(w.principal, Some(7));
    assert_eq!(
        w.inbound,
        vec![Inbound::Request {
            stream: 1,
            command: Command::Register
        }]
    );
    Ok(())
}

#[when(
    "a client's SCRAM proof does not verify, once for a principal that exists and once for a name no principal holds"
)]
async fn scram_refused(w: &mut Steps) -> TestResult {
    for (name, password) in [("alice", "wrong"), ("nobody", "pw")] {
        let mut c = Conversation::new(NetworkFaults::none(), authenticating(4), None).await?;
        let Err((code, frame)) = c.authenticate(name, password).await? else {
            return Err(TestError::new(format!("{name}'s exchange is refused")));
        };
        w.errors.push((seen(&frame)?.0, code, frame));
        assert!(c.connection.is_closing());
    }
    Ok(())
}

#[then(
    "each is refused with code 19 on stream 0 and the connection closed, and the two Error frames are the same bytes"
)]
fn refused_alike(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.errors.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        vec![
            (0, ErrorCode::AuthenticationFailed),
            (0, ErrorCode::AuthenticationFailed)
        ]
    );
    assert_eq!(w.errors[0].2, w.errors[1].2);
    Ok(())
}

#[when("a client sends Begin after Welcome and before the final Authenticate")]
async fn begin_before_auth(w: &mut Steps) -> TestResult {
    let mut c = Conversation::new(NetworkFaults::none(), authenticating(4), None).await?;
    c.send(1, &Begin::new(0, 0, false)).await?;
    w.inbound.push(c.server_reads().await?);
    c.server_sends(usize::MAX).await?;
    w.errors = errors(&c.client_frames().await?)?;
    Ok(())
}

#[then("the server answers code 20 on stream 0 and closes the connection")]
fn unauthenticated(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.inbound,
        vec![Inbound::Closing {
            code: Some(ErrorCode::Unauthenticated)
        }]
    );
    assert_eq!(
        w.errors.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        vec![(0, ErrorCode::Unauthenticated)]
    );
    Ok(())
}

#[when("a client sends Hello and then nothing for longer than the authentication timeout")]
async fn auth_timeout(w: &mut Steps) -> TestResult {
    let mut c = Conversation::new(NetworkFaults::none(), authenticating(4), None).await?;
    let deadline = c
        .connection
        .next_deadline()
        .context("the authentication's deadline")?;
    assert_eq!(deadline, c.now + AUTHENTICATION_TIMEOUT);
    w.inbound.extend(c.connection.handle_timeout(deadline));
    c.server_sends(usize::MAX).await?;
    w.errors = errors(&c.client_frames().await?)?;
    Ok(())
}

#[when(
    "a connection whose credential expires runs a new exchange as the same principal before the expiry, while one of its reads streams"
)]
async fn renewed(w: &mut Steps) -> TestResult {
    let lifetime = Duration::from_secs(60);
    let mut c = serving(1, Some(lifetime)).await?;
    let first = c.now;
    c.send(1, &Begin::new(0, 0, false)).await?;
    c.server_takes().await?;
    c.send(3, &read(1)).await?;
    c.server_takes().await?;
    c.result(3, 3)?;
    c.server_sends(usize::MAX).await?;
    w.seen = c.client_reads().await?;
    c.now = first + Duration::from_secs(50);
    w.principal = c.authenticate("alice", "pw").await?.ok();
    c.now = first + lifetime + Duration::from_secs(1);
    w.inbound.extend(c.connection.handle_timeout(c.now));
    c.send(3, &Credit::new(2)).await?;
    c.server_takes().await?;
    c.send(7, &Register::new(0)).await?;
    w.inbound.push(c.server_reads().await?);
    c.server_sends(usize::MAX).await?;
    w.later = c.client_reads().await?;
    Ok(())
}

#[then("the read continues, and the connection serves requests past the first expiry")]
fn renewal_served(w: &mut Steps) -> TestResult {
    assert_eq!(w.principal, Some(7));
    assert_eq!(w.seen, vec![(3, 13, 0)]);
    assert_eq!(
        w.inbound,
        vec![Inbound::Request {
            stream: 7,
            command: Command::Register
        }],
        "no deadline passed at the first expiry"
    );
    assert!(
        w.later.contains(&(3, 13, FLAG_LAST)),
        "the read continued: {:?}",
        w.later
    );
    Ok(())
}

#[when("a connection's new exchange ends as another principal")]
async fn renewed_as_bob(w: &mut Steps) -> TestResult {
    let mut c = serving(4, None).await?;
    let Err((code, frame)) = c.authenticate("bob", "pw").await? else {
        return Err(TestError::new(
            "bob's exchange on alice's connection is refused",
        ));
    };
    w.errors.push((seen(&frame)?.0, code, frame));
    w.principal = c.connection.is_closing().then_some(0);
    Ok(())
}

#[then("the server answers code 19 on stream 0 and closes the connection")]
fn other_principal_refused(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.errors.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        vec![(0, ErrorCode::AuthenticationFailed)]
    );
    assert_eq!(w.principal, Some(0));
    Ok(())
}

#[when("a client connects to a server configured without authentication")]
async fn open_server(w: &mut Steps) -> TestResult {
    let mut c = Conversation::new(NetworkFaults::none(), welcome(4), None).await?;
    w.welcome = c.connection.welcome().cloned();
    c.send(1, &Register::new(0)).await?;
    w.inbound.push(c.server_reads().await?);
    w.principal = c.connection.principal();
    Ok(())
}

#[then(
    "the Welcome says no authentication is required and lists no mechanism, and the client's requests are served without an exchange"
)]
fn open_served(w: &mut Steps) -> TestResult {
    let welcome = w.welcome.clone().context("the client's Welcome")?;
    assert!(!welcome.authenticates());
    assert!(welcome.mechanisms().is_empty());
    assert_eq!(
        w.inbound,
        vec![Inbound::Request {
            stream: 1,
            command: Command::Register
        }]
    );
    assert_eq!(w.principal, Some(0), "the anonymous principal");
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
