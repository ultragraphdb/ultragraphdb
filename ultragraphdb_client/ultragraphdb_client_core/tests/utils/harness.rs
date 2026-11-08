/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The core driven against the test server, synchronously: frames carried as bytes, over a simulated network with
//! its faults, each side assembling frames from what arrives with the transport's framer, one per connection. Time
//! is a value the harness moves on when nothing else can, and the wall clock follows it. Every loop is bounded, so a
//! defect fails rather than hangs.

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use ultragraphdb_client_core::{
    ClientConfig, ClientCore, ClientError, ClientErrorEnum, Committed, Pending, Reply, Session,
    Statement, TEST_NAME, TEST_PASSWORD, TestServer, Transaction,
};
use ultragraphdb_protocol_scram::{MECHANISM, ScramError, ScramLogin};
use ultragraphdb_protocol_transport::{
    Anonymous, BufferPool, FrameStep, Framer, Login, NetworkFaults, ReceivedFrame, SimNetwork,
    SimStream, TransportError,
};
use ultragraphdb_protocol_wire::{
    Describe, HEADER_SIZE, Header, HeaderView, MAX_FRAME_SIZE, Welcome,
};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

/// The cluster the client and the test server name, so each end's framer accepts the other's frames.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
/// The release the client's Hello and the server's Welcome name.
pub const RELEASE: u32 = 0x0005_0000;
/// The wall clock at the harness's start, microseconds of Unix time: 2027-01-15.
pub const UNIX: u64 = 1_800_000_000_000_000;
/// The most steps any conversation of these tests takes, and the most frames one side hands over in one step: the
/// longest conversation measured takes under thirty, so a defect that keeps a conversation moving fails within
/// seconds rather than hanging the suite.
const STEPS: usize = 200;

/// The bytes arrived at one end, cut into whole frames.
#[derive(Debug, Default)]
struct Bytes {
    data: VecDeque<u8>,
}

impl Bytes {
    fn take(&mut self, stream: &SimStream) {
        let mut cx = Context::from_waker(Waker::noop());
        while let Poll::Ready(Some(chunk)) = stream.poll_chunk(&mut cx, 1 << 20) {
            self.data.extend(chunk);
        }
    }

    /// The next whole frame, by its header's size; a frame not yet whole stays.
    fn frame(&mut self) -> Option<Vec<u8>> {
        if self.data.len() < HEADER_SIZE {
            return None;
        }
        let head: Vec<u8> = self.data.iter().take(HEADER_SIZE).copied().collect();
        let size = HeaderView::new(&head)
            .map(|h| h.size() as usize)
            .ok()?
            .max(HEADER_SIZE);
        if self.data.len() < size {
            return None;
        }
        Some(self.data.drain(..size).collect())
    }
}

/// `bytes`, one whole frame, through `framer` into a buffer of `pool`, as a client's shell hands a frame over.
pub fn frame_through(
    framer: &mut Framer,
    pool: &BufferPool,
    bytes: &[u8],
) -> Result<ReceivedFrame, TransportError> {
    let mut buffer = pool.acquire()?;
    framer.reset();
    loop {
        let mut data = buffer.take();
        let want = framer.want().min(bytes.len());
        data.clear();
        data.extend_from_slice(&bytes[..want]);
        let short = data.len() < framer.want();
        let got = data.len();
        buffer.restore(data);
        if short {
            return Err(framer.closed(got));
        }
        match framer.advance(&buffer)? {
            FrameStep::Read(_) => {}
            FrameStep::Frame => return Ok(framer.finish(buffer)),
        }
    }
}

/// A connection's first frame from the server, through a framer of its own: the frame the client expects at
/// sequence 1.
pub fn received(pool: &BufferPool, bytes: &[u8]) -> Result<ReceivedFrame, TransportError> {
    frame_through(&mut Framer::new(CLUSTER, MAX_FRAME_SIZE), pool, bytes)
}

/// A point in time to start from. The core never reads a clock; the tests move this value on themselves, so the
/// clock is read once, here, for an origin only.
#[expect(
    clippy::disallowed_methods,
    reason = "an `Instant` is made only by reading the clock, so the tests' origin reads it once"
)]
pub fn start() -> Instant {
    Instant::now()
}

/// The Welcome of the tests' server: protocol 1, frames up to 1 MiB, 64 streams, an initial credit of 4 frames.
pub fn welcome() -> Welcome {
    Welcome::new(1, RELEASE, "test")
        .with_max_frame(1 << 20)
        .with_max_streams(64)
        .with_initial_credit(4)
}

/// The Welcome of a server asking for SCRAM-SHA-256.
pub fn scram_welcome() -> Welcome {
    welcome().with_mechanisms([MECHANISM])
}

/// The client's configuration in the tests: a 1 s timeout, a commit retried 10 ms apart, a credit of 2 frames.
pub fn config() -> ClientConfig {
    ClientConfig::new(CLUSTER, "ultragraph-test")
        .with_release(RELEASE)
        .with_timeout(Duration::from_secs(1))
        .with_retry_delay(Duration::from_millis(10))
        .with_credit(2)
}

/// The test server of [`welcome`].
pub fn server() -> TestServer {
    TestServer::new(CLUSTER, welcome())
}

/// The client nonces of the tests: one fixed nonce, so a conversation replays.
pub fn nonce() -> Result<String, ScramError> {
    Ok("client-nonce".to_string())
}

/// The test server's principal's login by SCRAM-SHA-256.
pub type TestLogin = ScramLogin<fn() -> Result<String, ScramError>>;

/// The test principal's login, with the fixed client nonce of [`nonce`].
pub fn login() -> TestResult<TestLogin> {
    ScramLogin::new(
        TEST_NAME,
        TEST_PASSWORD,
        nonce as fn() -> Result<String, ScramError>,
    )
    .context("the test principal's name and password prepare")
}

/// The client's core, the test server, and the simulated connection between them.
pub struct Harness<L: Login = Anonymous> {
    /// The client's core under test.
    pub core: ClientCore<L>,
    /// The server the core talks to.
    pub server: TestServer,
    /// The harness's time, which both ends are handed and which moves only when the harness moves it.
    pub now: Instant,
    /// The buffers the client's frames are read into.
    pub pool: BufferPool,
    /// The simulated network carrying the connections, with its faults.
    pub network: SimNetwork,
    origin: Instant,
    client: SimStream,
    server_end: SimStream,
    at_client: Bytes,
    at_server: Bytes,
    framer: Framer,
    /// The server's frames a test made on this connection, numbered after the server's own.
    forged: u128,
    /// While set, every frame the server would send is lost.
    pub blackhole: bool,
    /// The connections made.
    pub connections: u32,
}

impl Harness<Anonymous> {
    /// The core of [`config`] before its first connection to `server`, over a network without faults.
    pub fn new(server: TestServer) -> Self {
        Self::with(config(), server, NetworkFaults::none(), 1, Anonymous)
    }

    /// The core connected and welcomed.
    pub fn ready(server: TestServer) -> TestResult<Self> {
        let mut h = Self::new(server);
        h.connect()?;
        h.until(|h| h.core.is_ready())?;
        Ok(h)
    }
}

impl Harness<TestLogin> {
    /// The core connected to a server asking for SCRAM-SHA-256, authenticated as the test principal.
    pub fn authenticated(server: TestServer) -> TestResult<Self> {
        let mut h = Self::with(config(), server, NetworkFaults::none(), 1, login()?);
        h.connect()?;
        h.until(|h| h.core.is_ready())?;
        Ok(h)
    }
}

impl<L: Login> Harness<L> {
    /// The core of `config` with `login` before its first connection to `server`, over a network with `faults` drawn
    /// from `seed`.
    pub fn with(
        config: ClientConfig,
        server: TestServer,
        faults: NetworkFaults,
        seed: u64,
        login: L,
    ) -> Self {
        let network = SimNetwork::new(seed, faults);
        let (client, server_end) = network.connect();
        let now = start();
        Self {
            core: ClientCore::new(config, login),
            server,
            now,
            pool: BufferPool::new(64, 1 << 20),
            network,
            origin: now,
            client,
            server_end,
            at_client: Bytes::default(),
            at_server: Bytes::default(),
            framer: Framer::new(CLUSTER, MAX_FRAME_SIZE),
            forged: 0,
            blackhole: false,
            connections: 0,
        }
    }

    /// The wall clock at the harness's time, microseconds of Unix time.
    pub fn unix(&self) -> u64 {
        UNIX + self.now.saturating_duration_since(self.origin).as_micros() as u64
    }

    /// A new connection: the core's handshake begun, the server and the client's framer reset for it.
    pub fn connect(&mut self) -> TestResult {
        self.client.shutdown();
        self.server_end.shutdown();
        let (client, server_end) = self.network.connect();
        self.client = client;
        self.server_end = server_end;
        self.at_client = Bytes::default();
        self.at_server = Bytes::default();
        self.framer = Framer::new(CLUSTER, MAX_FRAME_SIZE);
        self.forged = 0;
        let unix = self.unix();
        self.server.connection(self.now, unix);
        self.connections += 1;
        self.core
            .connect(self.now, unix)
            .context("the core connects")
    }

    /// A frame of the server's a test made, numbered as the connection's next: after every frame the server sent and
    /// every one made before it.
    pub fn numbered(&mut self, frame: &[u8]) -> TestResult<Vec<u8>> {
        self.forged += 1;
        let mut stamped = frame.to_vec();
        Header::stamp_sequence(&mut stamped, self.server.sent() + self.forged)
            .context("a whole frame")?;
        Ok(stamped)
    }

    /// A frame of the server delivered to the core as it stands, through the client's framer: the core's answer, or
    /// the test's failure when `bytes` is no whole frame.
    pub fn deliver(&mut self, bytes: &[u8]) -> TestResult<Result<(), ClientError>> {
        let frame = frame_through(&mut self.framer, &self.pool, bytes).context("a whole frame")?;
        Ok(self.core.receive(frame, self.now))
    }

    /// The core's next frame sent twice, as a network that repeats a frame delivers it.
    pub fn send_twice(&mut self) {
        if let Some(frame) = self.core.transmit() {
            self.client.send(frame.clone());
            self.client.send(frame);
        }
    }

    /// One round: the core's frames to the server, the server's to the core; whether anything moved. A side handing
    /// over more than [`STEPS`] frames in one round fails the test.
    pub fn step(&mut self) -> TestResult<bool> {
        let mut moved = false;
        for sent in 0..=STEPS {
            let Some(frame) = self.core.transmit() else {
                break;
            };
            if sent == STEPS {
                return Err(TestError::new(format!(
                    "the client sent {STEPS} frames in one round"
                )));
            }
            self.client.send(frame);
            moved = true;
        }
        self.network.run_until_idle();
        self.at_server.take(&self.server_end);
        while let Some(frame) = self.at_server.frame() {
            self.server.receive(&frame, self.now);
            moved = true;
        }
        for sent in 0..=STEPS {
            let Some(frame) = self.server.transmit() else {
                break;
            };
            if sent == STEPS {
                return Err(TestError::new(format!(
                    "the server sent {STEPS} frames in one round"
                )));
            }
            if !self.blackhole {
                self.server_end.send(frame);
            }
            moved = true;
        }
        if self.server.dropped() {
            self.server.connection(self.now, self.unix());
            self.core.disconnected(TransportError::closed(), self.now);
            moved = true;
        }
        self.network.run_until_idle();
        self.at_client.take(&self.client);
        while let Some(bytes) = self.at_client.frame() {
            let frame =
                frame_through(&mut self.framer, &self.pool, &bytes).context("a whole frame")?;
            if let Some(welcome) = self.core.welcome() {
                self.framer.set_protocol(welcome.protocol());
            }
            // A refusal gives the connection up, and the requests waiting on it fail with it as their cause, which
            // is what the tests read.
            drop(self.core.receive(frame, self.now));
            moved = true;
        }
        if self.server.is_closing() && self.core.is_connected() {
            self.core.disconnected(TransportError::closed(), self.now);
            moved = true;
        }
        Ok(moved)
    }

    /// Rounds until `done` holds: when nothing moves, time moves to the earlier of the core's next deadline and the
    /// server's, and a core that has lost its connection is connected again. A core whose deadline is still due once
    /// it handled the time fails the test, as a shell calling it at its deadline would never wait.
    pub fn until(&mut self, mut done: impl FnMut(&mut Self) -> bool) -> TestResult {
        let mut steps = 0;
        loop {
            if steps >= STEPS {
                return Err(TestError::new(format!("not done after {STEPS} steps")));
            }
            steps += 1;
            if done(self) {
                return Ok(());
            }
            if self.core.needs_connection() {
                self.connect()?;
                continue;
            }
            if !self.step()? {
                let due = [self.core.next_deadline(), self.server.next_deadline()];
                let Some(deadline) = due.into_iter().flatten().min() else {
                    if done(self) {
                        return Ok(());
                    }
                    return Err(TestError::new("nothing moves and nothing is due"));
                };
                self.now = self.now.max(deadline);
                self.server.handle_timeout(self.now);
                self.core.handle_timeout(self.now);
                if let Some(again) = self.core.next_deadline().filter(|at| *at <= self.now) {
                    return Err(TestError::new(format!(
                        "the core is due again at {again:?}, at or before the time it handled, {:?}",
                        self.now
                    )));
                }
            }
        }
    }

    /// The answer of `pending`.
    pub fn settle(&mut self, pending: Pending) -> TestResult<Result<Reply, ClientError>> {
        let mut answer = None;
        self.until(|h| {
            answer = h.core.take_reply(pending);
            answer.is_some()
        })?;
        answer.context("an answer")
    }
}

/// The session and request of an error saying a commit is in doubt.
pub fn in_doubt(error: &ClientError) -> Option<(u128, u64)> {
    if let ClientErrorEnum::InDoubt {
        session, request, ..
    } = &error.0
    {
        return Some((*session, *request));
    }
    None
}

/// `frames`, at least 1, as the core's grant takes it.
pub fn frames(frames: u32) -> TestResult<NonZeroU32> {
    NonZeroU32::new(frames).context("a grant of frames")
}

/// The session of a Register's `reply`.
pub fn registered(reply: Result<Reply, ClientError>) -> TestResult<Session> {
    let found = format!("{reply:?}");
    let reply = reply.ok();
    if let Some(Reply::Registered(session)) = reply {
        Some(session)
    } else {
        None
    }
    .ok_or(found)
    .context("a Register's session")
}

/// The transaction of a Begin's `reply`.
pub fn begun(reply: Result<Reply, ClientError>) -> TestResult<Transaction> {
    let found = format!("{reply:?}");
    let reply = reply.ok();
    if let Some(Reply::Begun(tx)) = reply {
        Some(tx)
    } else {
        None
    }
    .ok_or(found)
    .context("a Begin's transaction")
}

/// The answer of a Commit's `reply`.
pub fn committed(reply: Result<Reply, ClientError>) -> TestResult<Committed> {
    let found = format!("{reply:?}");
    let reply = reply.ok();
    if let Some(Reply::Committed(committed)) = reply {
        Some(committed)
    } else {
        None
    }
    .ok_or(found)
    .context("a Commit's answer")
}

/// The answer of a Drop's `reply`.
pub fn dropped(reply: Result<Reply, ClientError>) -> TestResult<Committed> {
    let found = format!("{reply:?}");
    let reply = reply.ok();
    if let Some(Reply::Dropped(dropped)) = reply {
        Some(dropped)
    } else {
        None
    }
    .ok_or(found)
    .context("a Drop's answer")
}

/// The statement of a Prepare's `reply`.
pub fn prepared(reply: Result<Reply, ClientError>) -> TestResult<Statement> {
    let found = format!("{reply:?}");
    let reply = reply.ok();
    if let Some(Reply::Prepared(statement)) = reply {
        Some(statement)
    } else {
        None
    }
    .ok_or(found)
    .context("a Prepare's statement")
}

/// The answer of a Describe's `reply`.
pub fn described(reply: Result<Reply, ClientError>) -> TestResult<Describe> {
    let found = format!("{reply:?}");
    let reply = reply.ok();
    if let Some(Reply::Described(describe)) = reply {
        Some(describe)
    } else {
        None
    }
    .ok_or(found)
    .context("a Describe's answer")
}
