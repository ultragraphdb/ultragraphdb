/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The core driven against the test server, synchronously: frames carried as bytes, over a simulated network with
//! its faults, each side assembling frames from what arrives with the transport's framer, one per connection. Time
//! is a value the harness moves on when nothing else can, and the wall clock follows it. Every loop is bounded, so a
//! defect fails rather than hangs.

use std::collections::VecDeque;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use ultragraphdb_client_core::{
    ClientConfig, ClientCore, ClientError, Pending, Reply, TEST_NAME, TEST_PASSWORD, TestServer,
};
use ultragraphdb_protocol_scram::{MECHANISM, ScramError, ScramLogin};
use ultragraphdb_protocol_transport::{
    Anonymous, BufferPool, FrameStep, Framer, Login, NetworkFaults, ReceivedFrame, SimNetwork,
    SimStream, TransportError,
};
use ultragraphdb_protocol_wire::{HEADER_SIZE, Header, HeaderView, MAX_FRAME_SIZE, Welcome};

pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
pub const RELEASE: u32 = 0x0005_0000;
/// The wall clock at the harness's start, microseconds of Unix time: 2027-01-15.
pub const UNIX: u64 = 1_800_000_000_000_000;
/// The most steps any conversation of these tests takes.
const STEPS: usize = 20_000;

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
#[allow(clippy::disallowed_methods)]
pub fn start() -> Instant {
    Instant::now()
}

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

pub fn login() -> TestLogin {
    ScramLogin::new(
        TEST_NAME,
        TEST_PASSWORD,
        nonce as fn() -> Result<String, ScramError>,
    )
    .expect("the test principal's name and password prepare")
}

/// The client's core, the test server, and the simulated connection between them.
pub struct Harness<L: Login = Anonymous> {
    pub core: ClientCore<L>,
    pub server: TestServer,
    pub now: Instant,
    pub pool: BufferPool,
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
    pub fn new(server: TestServer) -> Self {
        Self::with(config(), server, NetworkFaults::none(), 1, Anonymous)
    }

    /// The core connected and welcomed.
    pub fn ready(server: TestServer) -> Self {
        let mut h = Self::new(server);
        h.connect();
        h.until(|h| h.core.is_ready());
        h
    }
}

impl Harness<TestLogin> {
    /// The core connected to a server asking for SCRAM-SHA-256, authenticated as the test principal.
    pub fn authenticated(server: TestServer) -> Self {
        let mut h = Self::with(config(), server, NetworkFaults::none(), 1, login());
        h.connect();
        h.until(|h| h.core.is_ready());
        h
    }
}

impl<L: Login> Harness<L> {
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
    pub fn connect(&mut self) {
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
            .expect("the core connects");
    }

    /// A frame of the server's a test made, numbered as the connection's next: after every frame the server sent and
    /// every one made before it.
    pub fn numbered(&mut self, frame: &[u8]) -> Vec<u8> {
        self.forged += 1;
        let mut frame = frame.to_vec();
        Header::stamp_sequence(&mut frame, self.server.sent() + self.forged)
            .expect("a whole frame");
        frame
    }

    /// A frame of the server delivered to the core as it stands, through the client's framer.
    pub fn deliver(&mut self, bytes: &[u8]) -> Result<(), ClientError> {
        let frame = frame_through(&mut self.framer, &self.pool, bytes).expect("a whole frame");
        self.core.receive(frame, self.now)
    }

    /// The core's next frame sent twice, as a network that repeats a frame delivers it.
    pub fn send_twice(&mut self) {
        if let Some(frame) = self.core.transmit() {
            self.client.send(frame.clone());
            self.client.send(frame);
        }
    }

    /// One round: the core's frames to the server, the server's to the core; whether anything moved.
    pub fn step(&mut self) -> bool {
        let mut moved = false;
        while let Some(frame) = self.core.transmit() {
            self.client.send(frame);
            moved = true;
        }
        self.network.run_until_idle();
        self.at_server.take(&self.server_end);
        while let Some(frame) = self.at_server.frame() {
            self.server.receive(&frame, self.now);
            moved = true;
        }
        while let Some(frame) = self.server.transmit() {
            if !self.blackhole {
                self.server_end.send(frame);
            }
            moved = true;
        }
        if self.server.dropped() {
            self.server.connection(self.now, self.unix());
            self.core.disconnected(TransportError::Closed(), self.now);
            moved = true;
        }
        self.network.run_until_idle();
        self.at_client.take(&self.client);
        while let Some(frame) = self.at_client.frame() {
            let frame = frame_through(&mut self.framer, &self.pool, &frame).expect("a whole frame");
            if let Some(welcome) = self.core.welcome() {
                self.framer.set_protocol(welcome.protocol());
            }
            let _ = self.core.receive(frame, self.now);
            moved = true;
        }
        if self.server.is_closing() && self.core.is_connected() {
            self.core.disconnected(TransportError::Closed(), self.now);
            moved = true;
        }
        moved
    }

    /// Rounds until `done` holds: when nothing moves, time moves to the core's next deadline, or the server's when
    /// the core has none, and a core that has lost its connection is connected again.
    pub fn until(&mut self, mut done: impl FnMut(&mut Self) -> bool) {
        for _ in 0..STEPS {
            if done(self) {
                return;
            }
            if self.core.needs_connection() {
                self.connect();
                continue;
            }
            if !self.step() {
                match self.core.next_deadline().or(self.server.next_deadline()) {
                    Some(deadline) => {
                        self.now = self.now.max(deadline);
                        self.server.handle_timeout(self.now);
                        self.core.handle_timeout(self.now);
                    }
                    None => {
                        if done(self) {
                            return;
                        }
                        panic!("nothing moves and nothing is due");
                    }
                }
            }
        }
        panic!("not done after {STEPS} steps");
    }

    /// The answer of `pending`.
    pub fn settle(&mut self, pending: Pending) -> Result<Reply, ClientError> {
        let mut answer = None;
        self.until(|h| {
            answer = h.core.take_reply(pending);
            answer.is_some()
        });
        answer.expect("an answer")
    }
}
