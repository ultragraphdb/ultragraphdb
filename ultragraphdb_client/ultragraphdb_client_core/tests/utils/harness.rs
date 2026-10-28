/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The core driven against the scripted server, synchronously: frames carried as bytes, over a simulated network with
//! its faults, each side assembling frames from what arrives with the transport's framer. Time is a value the harness
//! moves on when nothing else can. Every loop is bounded, so a defect fails rather than hangs.

use std::collections::VecDeque;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use ultragraphdb_client_core::{
    ClientConfig, ClientCore, ClientError, Pending, Reply, ScriptedServer, ServerScript,
};
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, NetworkFaults, ReceivedFrame, SimNetwork, SimStream,
    TransportError,
};
use ultragraphdb_protocol_wire::{HEADER_SIZE, HeaderView, Welcome};

pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
pub const RELEASE: u32 = 0x0005_0000;
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

/// A frame's bytes as the client's shell hands them over: through the framer, into a pooled buffer.
pub fn received(pool: &BufferPool, bytes: &[u8]) -> Result<ReceivedFrame, TransportError> {
    let mut framer = Framer::new(CLUSTER);
    let mut buffer = pool.acquire()?;
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

/// The client's core, the scripted server, and the simulated connection between them.
pub struct Harness {
    pub core: ClientCore,
    pub server: ScriptedServer,
    pub now: Instant,
    pub pool: BufferPool,
    pub network: SimNetwork,
    client: SimStream,
    server_end: SimStream,
    at_client: Bytes,
    at_server: Bytes,
    /// While set, every frame the server would send is lost.
    pub blackhole: bool,
    /// The connections made.
    pub connections: u32,
}

/// A point in time to start from. The core never reads a clock; the tests move this value on themselves, so the
/// clock is read once, here, for an origin only.
#[allow(clippy::disallowed_methods)]
pub fn start() -> Instant {
    Instant::now()
}

pub fn welcome() -> Welcome {
    Welcome::new(1, RELEASE, 1 << 20, 64, 4, "scripted")
}

pub fn config() -> ClientConfig {
    ClientConfig::new(CLUSTER, "ultragraph-test")
        .with_release(RELEASE)
        .with_timeout(Duration::from_secs(1))
        .with_retry_delay(Duration::from_millis(10))
        .with_credit(2)
}

impl Harness {
    pub fn new(script: ServerScript) -> Self {
        Self::with(config(), welcome(), script, NetworkFaults::none(), 1)
    }

    pub fn with(
        config: ClientConfig,
        welcome: Welcome,
        script: ServerScript,
        faults: NetworkFaults,
        seed: u64,
    ) -> Self {
        let network = SimNetwork::new(seed, faults);
        let (client, server_end) = network.connect();
        Self {
            core: ClientCore::new(config),
            server: ScriptedServer::new(CLUSTER, welcome, script),
            now: start(),
            pool: BufferPool::new(64, 1 << 20),
            network,
            client,
            server_end,
            at_client: Bytes::default(),
            at_server: Bytes::default(),
            blackhole: false,
            connections: 0,
        }
    }

    /// A new connection: the core's handshake begun, the server reset for it.
    pub fn connect(&mut self) {
        self.client.shutdown();
        self.server_end.shutdown();
        let (client, server_end) = self.network.connect();
        self.client = client;
        self.server_end = server_end;
        self.at_client = Bytes::default();
        self.at_server = Bytes::default();
        self.server.connection();
        self.connections += 1;
        self.core.connect(self.now).expect("the core connects");
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
            self.server.receive(&frame);
            moved = true;
        }
        while let Some(frame) = self.server.transmit() {
            if !self.blackhole {
                self.server_end.send(frame);
            }
            moved = true;
        }
        if self.server.dropped() {
            self.server.connection();
            self.core.disconnected(TransportError::Closed(), self.now);
            moved = true;
        }
        self.network.run_until_idle();
        self.at_client.take(&self.client);
        while let Some(frame) = self.at_client.frame() {
            let frame = received(&self.pool, &frame).expect("a whole frame");
            let _ = self.core.receive(frame, self.now);
            moved = true;
        }
        moved
    }

    /// Rounds until `done` holds: when nothing moves, time moves to the core's next deadline, and a core that has
    /// lost its connection is connected again.
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
                match self.core.next_deadline() {
                    Some(deadline) => {
                        self.now = self.now.max(deadline);
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

    /// The core connected and welcomed.
    pub fn ready(script: ServerScript) -> Self {
        let mut h = Self::new(script);
        h.connect();
        h.until(|h| h.core.is_ready());
        h
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
