/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conversations of the conformance suite over the bus: a client and a server connection on the simulated
//! network, the server's frames scheduled by `Outbound`.

use cucumber::{World, then, when};
use ultragraphdb_protocol_bus::{
    BufferPool, Connection, NetworkFaults, Outbound, SimNetwork, SimStream,
};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Cancel, Command, Commit, Credit, CreditView, FLAG_LAST, Header, Hello,
    MAX_FRAME_SIZE, ResultBatch, Welcome,
};

// The bounded executor of the unit tests, shared by path.
mod utils {
    #[path = "ready.rs"]
    pub mod ready;
}

use crate::utils::ready::ready;

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A frame the client received: its stream, command and flags.
type Seen = (u64, u16, u8);

#[derive(Debug, Default, World)]
struct Steps {
    seen: Vec<Seen>,
    later: Vec<Seen>,
    queued: usize,
    welcome: Option<Welcome>,
    welcomed: Option<Welcome>,
}

/// Both ends of a connection after the handshake, the server's scheduler, and the client's end for its probe.
struct Conversation {
    network: SimNetwork,
    client: Connection<SimStream>,
    server: Connection<SimStream>,
    probe: SimStream,
    outbound: Outbound,
    pool: BufferPool,
}

impl Conversation {
    async fn new(faults: NetworkFaults, welcome: Welcome) -> Self {
        let network = SimNetwork::new(17, faults);
        let (client, server) = network.connect();
        let probe = client.clone();
        let mut client = Connection::new(client, CLUSTER);
        let mut server = Connection::new(server, CLUSTER);
        let pool = BufferPool::new(64, MAX_FRAME_SIZE);
        let (hello, answered) = ready(futures::future::join(
            client.hello(&pool, &Hello::new(1, 1, 0, "client")),
            server.welcome(&pool, &welcome),
        ));
        let welcome = hello.expect("the client is welcomed");
        answered.expect("the server welcomes");
        let outbound = Outbound::new(welcome.max_streams(), welcome.initial_credit());
        Self {
            network,
            client,
            server,
            probe,
            outbound,
            pool,
        }
    }

    /// The server queues a result of `frames` frames on `stream`, the last flagged.
    fn result(&mut self, stream: u64, frames: u32) {
        self.outbound.open(stream).unwrap();
        for i in 0..frames {
            let flags = if i + 1 == frames { FLAG_LAST } else { 0 };
            let frame = ResultBatch::new(1)
                .frame(
                    Header::new(CLUSTER, Command::Result)
                        .with_stream(stream)
                        .with_flags(flags),
                )
                .unwrap();
            self.outbound.push(stream, frame).unwrap();
        }
    }

    async fn client_sends(&mut self, frame: Vec<u8>) {
        ready(self.client.write_frame(frame)).unwrap();
    }

    /// The server reads one frame and acts on Credit or Cancel.
    async fn server_reads(&mut self) {
        let received = ready(self.server.read_frame(&self.pool)).unwrap();
        let frame = received.view().unwrap();
        match frame.command() {
            Command::Credit => {
                let credit = CreditView::from_frame(&frame).unwrap();
                self.outbound
                    .grant(frame.header().stream(), credit.frames());
            }
            Command::Cancel => {
                self.outbound.cancel(frame.header().stream());
            }
            other => panic!("the server does not serve {other:?} here"),
        }
    }

    /// The server sends up to `most` frames its scheduler lets go.
    async fn server_sends(&mut self, most: usize) {
        for _ in 0..most {
            match self.outbound.next_frame() {
                Some((_, frame)) => ready(self.server.write_frame(frame)).unwrap(),
                None => break,
            }
        }
    }

    /// Every frame that has reached the client.
    async fn client_reads(&mut self) -> Vec<Seen> {
        self.network.run_until_idle();
        let mut seen = Vec::new();
        while self.probe.pending() > 0 {
            let frame = ready(self.client.read_frame(&self.pool)).unwrap();
            seen.push((frame.stream(), frame.command(), frame.flags()));
        }
        seen
    }
}

fn welcome(initial_credit: u32) -> Welcome {
    Welcome::new(1, 0, 1 << 20, 16, initial_credit, "server")
}

async fn credit_conversation(w: &mut Steps, faults: NetworkFaults) {
    let mut c = Conversation::new(faults, welcome(0)).await;
    c.result(1, 10);
    c.server_sends(usize::MAX).await;
    assert!(c.client_reads().await.is_empty(), "no credit, no frame");
    c.client_sends(
        Credit::new(2)
            .frame(Header::new(CLUSTER, Command::Credit).with_stream(1))
            .unwrap(),
    )
    .await;
    c.server_reads().await;
    c.server_sends(usize::MAX).await;
    w.seen = c.client_reads().await;
    w.queued = c.outbound.queued(1);
    c.client_sends(
        Credit::new(8)
            .frame(Header::new(CLUSTER, Command::Credit).with_stream(1))
            .unwrap(),
    )
    .await;
    c.server_reads().await;
    c.server_sends(usize::MAX).await;
    w.later = c.client_reads().await;
}

#[when("a client grants credit for two frames of a result of ten")]
async fn credit_two(w: &mut Steps) {
    credit_conversation(w, NetworkFaults::none()).await;
}

#[when(
    "a client grants credit for two frames of a result of ten over a network that splits and delays frames"
)]
async fn credit_two_faulty(w: &mut Steps) {
    credit_conversation(
        w,
        NetworkFaults::none()
            .with_split(700_000)
            .with_delay(500_000, 6),
    )
    .await;
}

#[then("the server sends two frames and waits until more credit arrives")]
fn two_then_wait(w: &mut Steps) {
    assert_eq!(w.seen, vec![(1, 13, 0), (1, 13, 0)]);
    assert_eq!(w.queued, 8, "the other eight wait for credit");
    assert_eq!(w.later.len(), 8);
    assert!(w.later[..7].iter().all(|s| *s == (1, 13, 0)));
    assert_eq!(w.later[7], (1, 13, FLAG_LAST));
}

#[when("a client cancels a stream mid-result")]
async fn cancel_mid_result(w: &mut Steps) {
    let mut c = Conversation::new(NetworkFaults::none(), welcome(10)).await;
    c.result(1, 5);
    c.result(2, 5);
    c.server_sends(2).await;
    w.seen = c.client_reads().await;
    c.client_sends(
        Cancel
            .frame(Header::new(CLUSTER, Command::Cancel).with_stream(1))
            .unwrap(),
    )
    .await;
    c.server_reads().await;
    c.server_sends(usize::MAX).await;
    w.later = c.client_reads().await;
}

#[then("no further frame of that stream arrives, and other streams continue")]
fn cancelled(w: &mut Steps) {
    assert_eq!(w.seen, vec![(1, 13, 0), (2, 13, 0)]);
    assert!(
        w.later.iter().all(|(stream, _, _)| *stream == 2),
        "{:?}",
        w.later
    );
    assert_eq!(w.later.len(), 4);
    assert_eq!(w.later[3].2, FLAG_LAST);
}

#[when("a client speaking protocols 1 to 3 says Hello to a server speaking 1 to 2")]
async fn hello_one_to_three(w: &mut Steps) {
    let network = SimNetwork::new(5, NetworkFaults::none());
    let (client, server) = network.connect();
    let mut client = Connection::new(client, CLUSTER);
    let mut server = Connection::new(server, CLUSTER);
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let limits = Welcome::new(2, 0x0005_0000, 1 << 20, 32, 4, "server");
    let (hello, answered) = ready(futures::future::join(
        client.hello(&pool, &Hello::new(1, 3, 0x0005_0000, "client")),
        server.welcome(&pool, &limits),
    ));
    w.welcome = Some(hello.unwrap());
    w.welcomed = Some(answered.unwrap());
}

#[then("the server welcomes it at protocol 2 with its limits")]
fn welcomed_at_two(w: &mut Steps) {
    let welcome = w.welcome.clone().unwrap();
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
}

#[when("a client sends requests on streams 1 and 2 and the result of stream 2 is ready first")]
async fn two_streams(w: &mut Steps) {
    let mut c = Conversation::new(NetworkFaults::none().with_split(500_000), welcome(1)).await;
    c.outbound.open(1).unwrap();
    c.outbound.open(2).unwrap();
    let commit = |stream| {
        Commit::new(1, stream, 10 + stream, Vec::new())
            .frame(
                Header::new(CLUSTER, Command::Commit)
                    .with_stream(stream)
                    .with_flags(FLAG_LAST),
            )
            .unwrap()
    };
    // Stream 2's reply is ready first: the server queues it, sends what it may, then stream 1's.
    c.outbound.push(2, commit(2)).unwrap();
    c.server_sends(usize::MAX).await;
    c.outbound.push(1, commit(1)).unwrap();
    c.server_sends(usize::MAX).await;
    w.seen = c.client_reads().await;
}

#[then("the reply of stream 2 arrives before the reply of stream 1, each on its own stream")]
fn any_order(w: &mut Steps) {
    assert_eq!(w.seen, vec![(2, 5, FLAG_LAST), (1, 5, FLAG_LAST)]);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
