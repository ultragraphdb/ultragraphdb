/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scripted server on compio: each connection served until the client closes it or the script drops it, the
//! server's state kept across connections; and a connector over the simulated network whose server ends it accepts.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::poll_fn;
use std::io;
use std::rc::Rc;
use std::task::Poll;
use std::time::Duration;

use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_client_compio::Connect;
use ultragraphdb_client_core::{ClientConfig, ScriptedServer, ServerScript};
use ultragraphdb_protocol_transport::{BufferPool, Framer, SimNetwork, SimStream};
use ultragraphdb_protocol_transport_compio::{CompioSimStream, Connection};
use ultragraphdb_protocol_wire::{MAX_FRAME_SIZE, Welcome};

pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

pub fn config() -> ClientConfig {
    ClientConfig::new(CLUSTER, "ultragraph-test")
        .with_timeout(Duration::from_secs(2))
        .with_retry_delay(Duration::from_millis(5))
        .with_credit(2)
}

pub fn server(script: ServerScript) -> Rc<RefCell<ScriptedServer>> {
    Rc::new(RefCell::new(ScriptedServer::new(
        CLUSTER,
        Welcome::new(1, 0, "scripted")
            .with_max_frame(1 << 20)
            .with_max_streams(64)
            .with_initial_credit(1),
        script,
    )))
}

/// One connection served through the transport's compio connection, until the client closes or the script drops it.
pub async fn serve<S: AsyncRead + AsyncWrite>(server: &RefCell<ScriptedServer>, stream: S) {
    server.borrow_mut().connection();
    let pool = BufferPool::new(4, 1 << 20);
    let mut connection = Connection::new(stream, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
    loop {
        let frames: Vec<Vec<u8>> = std::iter::from_fn(|| server.borrow_mut().transmit()).collect();
        for frame in frames {
            if connection.write_frame(frame).await.is_err() {
                return;
            }
        }
        if server.borrow().dropped() {
            connection.shutdown().await;
            return;
        }
        let Ok(frame) = connection.read_frame(&pool).await else {
            return;
        };
        server.borrow_mut().receive(frame.bytes());
    }
}

/// Once polled pending, then ready: the other tasks run in between.
async fn yield_now() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}

/// Connections over the simulated network: each connect makes one and leaves its server end to be accepted.
#[derive(Debug, Clone)]
pub struct SimConnect {
    pub network: SimNetwork,
    pub accepted: Rc<RefCell<VecDeque<SimStream>>>,
}

impl SimConnect {
    pub fn new(network: SimNetwork) -> Self {
        Self {
            network,
            accepted: Rc::new(RefCell::new(VecDeque::new())),
        }
    }
}

impl Connect for SimConnect {
    type Stream = CompioSimStream;

    async fn connect(&mut self) -> io::Result<CompioSimStream> {
        let (client, server) = self.network.connect();
        self.accepted.borrow_mut().push_back(server);
        Ok(CompioSimStream::new(client))
    }
}

/// Serves every connection `connect` makes, one after another, for as long as the task lives.
pub async fn serve_sim(
    server: Rc<RefCell<ScriptedServer>>,
    accepted: Rc<RefCell<VecDeque<SimStream>>>,
) {
    loop {
        let next = accepted.borrow_mut().pop_front();
        match next {
            Some(stream) => serve(&server, CompioSimStream::new(stream)).await,
            None => yield_now().await,
        }
    }
}
