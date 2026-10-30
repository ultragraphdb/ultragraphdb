/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scripted server on tokio: each connection served until the client closes it or the script drops it, the
//! server's state kept across connections; and a connector over the simulated network whose server ends it accepts.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use ultragraphdb_client_core::{ClientConfig, ScriptedServer, ServerScript};
use ultragraphdb_client_tokio::{Connect, TokioSimStream};
use ultragraphdb_protocol_transport::{BufferPool, FrameStep, Framer, SimNetwork, SimStream};
use ultragraphdb_protocol_wire::Welcome;

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

/// One connection served: frames read whole, answered as the script says, until the client closes or the script
/// drops the connection.
pub async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    server: &RefCell<ScriptedServer>,
    mut stream: S,
) {
    server.borrow_mut().connection();
    let pool = BufferPool::new(4, 1 << 20);
    let mut framer = Framer::new(CLUSTER);
    loop {
        let frames: Vec<Vec<u8>> = std::iter::from_fn(|| server.borrow_mut().transmit()).collect();
        for frame in frames {
            if stream.write_all(&frame).await.is_err() {
                return;
            }
        }
        if server.borrow().dropped() {
            let _ = stream.shutdown().await;
            return;
        }
        let Ok(mut buffer) = pool.acquire() else {
            return;
        };
        let frame = loop {
            let mut bytes = buffer.take();
            let want = framer.want() - bytes.len();
            let read = (&mut stream).take(want as u64).read_buf(&mut bytes).await;
            buffer.restore(bytes);
            match read {
                Ok(0) | Err(_) => return,
                Ok(_) if buffer.len() < framer.want() => continue,
                Ok(_) => {}
            }
            match framer.advance(&buffer) {
                Ok(FrameStep::Read(_)) => {}
                Ok(FrameStep::Frame) => break framer.finish(buffer),
                Err(_) => return,
            }
        };
        server.borrow_mut().receive(frame.bytes());
    }
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
    type Stream = TokioSimStream;

    async fn connect(&mut self) -> io::Result<TokioSimStream> {
        let (client, server) = self.network.connect();
        self.accepted.borrow_mut().push_back(server);
        Ok(TokioSimStream::new(client))
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
            Some(stream) => serve(&server, TokioSimStream::new(stream)).await,
            None => tokio::task::yield_now().await,
        }
    }
}
