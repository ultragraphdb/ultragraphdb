/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::task::LocalSet;
use ultragraphdb_client_tokio::{Anonymous, Client, ClientError, Connect, TokioSimStream};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork, TransportError};

use crate::utils::serve::{SimConnect, config, serve_sim, server};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

#[test]
fn test_close_gives_the_connection_up() {
    let rt = runtime();
    let scripted = server();
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        client.connect().await.unwrap();
        assert!(client.core().is_ready());
        client.close().await;
        assert!(!client.core().is_ready());
        assert!(!client.core().is_connected());
    });
}

/// A simulated stream whose first read fails with `fault`, the rest served by the stream.
struct Faulty {
    stream: TokioSimStream,
    fault: Option<io::ErrorKind>,
}

impl AsyncRead for Faulty {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Some(kind) = self.fault.take() {
            return Poll::Ready(Err(kind.into()));
        }
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Faulty {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// Connections over the simulated network whose first connection's first read fails with `fault`.
struct FaultyConnect {
    inner: SimConnect,
    fault: Option<io::ErrorKind>,
}

impl Connect for FaultyConnect {
    type Stream = Faulty;

    async fn connect(&mut self) -> io::Result<Faulty> {
        let stream = self.inner.connect().await?;
        Ok(Faulty {
            stream,
            fault: self.fault.take(),
        })
    }
}

/// `client.connect()` over a connection whose first read fails with `fault`.
fn connect_with_fault(fault: io::ErrorKind) -> Result<(), ClientError> {
    let rt = runtime();
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = FaultyConnect {
        inner,
        fault: Some(fault),
    };
    LocalSet::new().block_on(&rt, async {
        tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        let connected = client.connect().await;
        assert_eq!(connected.is_ok(), client.core().is_ready());
        connected
    })
}

#[test]
fn test_an_interrupted_read_is_retried() {
    // A signal interrupting the read is no failure: the read is made again and the handshake completes.
    assert_eq!(connect_with_fault(io::ErrorKind::Interrupted), Ok(()));
}

#[test]
fn test_a_failed_read_fails_the_connection_with_its_error() {
    // A reset connection is the stream's failure, as `TransportError::Io` records it: its kind and its text.
    assert_eq!(
        connect_with_fault(io::ErrorKind::ConnectionReset),
        Err(ClientError::Transport(TransportError::Io(
            &io::ErrorKind::ConnectionReset.into()
        )))
    );
}
