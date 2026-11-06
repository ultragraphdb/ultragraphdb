/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::AsyncWrite;
use ultragraphdb_protocol_transport::{Address, BufferPool, Framer, Scheme, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::{Carrier, Connection, Listener};
use ultragraphdb_protocol_wire::{Hello, MAX_FRAME_SIZE, Welcome};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::bytes::{read_once, receive, send};
use crate::utils::socket::SocketDir;

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A listener at `address` and a carrier dialled to it, with the carrier it accepted.
async fn pair(address: &Address) -> TestResult<(Listener, Carrier, Carrier)> {
    let listener = Listener::bind(address).await?;
    let bound = listener.local_address()?;
    let (accepted, dialled) =
        futures::future::join(listener.accept(), Carrier::connect(&bound)).await;
    Ok((listener, accepted?, dialled?))
}

/// Bytes cross both ways between two carriers, and a shutdown reads as the end.
async fn round_trip(mut near: Carrier, mut far: Carrier) -> TestResult {
    send(&mut near, b"ping").await?;
    assert_eq!(receive(&mut far, 4).await?, b"ping");
    send(&mut far, b"pong").await?;
    assert_eq!(receive(&mut near, 4).await?, b"pong");
    near.shutdown().await.context("a shutdown")?;
    assert_eq!(read_once(&mut far).await?, 0);
    Ok(())
}

#[test]
fn test_a_tcp_carrier_keeps_alive_on_either_side() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let (_listener, accepted, dialled) = pair(&Address::any_port("127.0.0.1")?).await?;
        assert!(accepted.is_kept_alive()?, "accepted");
        assert!(dialled.is_kept_alive()?, "dialled");
        Ok(())
    })
}

#[test]
fn test_a_tcp_carrier_sends_each_frame_at_once_on_either_side() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let (_listener, accepted, dialled) = pair(&Address::any_port("127.0.0.1")?).await?;
        assert!(accepted.is_no_delay()?, "accepted");
        assert!(dialled.is_no_delay()?, "dialled");
        Ok(())
    })
}

#[test]
fn test_a_tcp_address_carries_bytes_both_ways() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let (listener, accepted, dialled) = pair(&Address::any_port("127.0.0.1")?).await?;
        let bound = listener.local_address()?;
        assert_eq!(bound.scheme(), Scheme::Tcp);
        assert_ne!(
            bound.host_port(),
            Some("127.0.0.1:0"),
            "the system chose a port"
        );
        assert!(matches!(
            (&accepted, &dialled),
            (Carrier::Tcp(_), Carrier::Tcp(_))
        ));
        round_trip(dialled, accepted).await
    })
}

#[test]
fn test_a_unix_address_carries_bytes_both_ways() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let address = dir.address("s.sock")?;
        let (listener, accepted, dialled) = pair(&address).await?;
        assert_eq!(listener.local_address()?, address);
        assert!(matches!(
            (&accepted, &dialled),
            (Carrier::Unix(_), Carrier::Unix(_))
        ));
        round_trip(dialled, accepted).await
    })
}

#[test]
fn test_a_connection_runs_its_handshake_over_a_unix_socket() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let (_listener, accepted, dialled) = pair(&dir.address("s.sock")?).await?;
        let mut server = Connection::new(accepted, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
        let mut client = Connection::new(dialled, Framer::new(CLUSTER, MAX_FRAME_SIZE));
        let pool = BufferPool::new(4, MAX_FRAME_SIZE);
        let welcome = Welcome::new(1, 0, "s")
            .with_max_streams(8)
            .with_initial_credit(2);
        let hello = Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c");
        let (client_side, server_side) =
            futures::future::join(client.hello(&pool, &hello), server.welcome(&pool, &welcome))
                .await;
        assert_eq!(client_side?, welcome);
        assert_eq!(server_side?, welcome);
        Ok::<(), TestError>(())
    })
}

#[test]
fn test_dialling_where_nothing_listens_fails_as_io() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let Err(error) = Carrier::connect(&dir.address("s.sock")?).await else {
            return Err(TestError::new(
                "a socket path nothing listens at was dialled",
            ));
        };
        // No socket file at the path: the system says it is not found.
        assert!(
            matches!(
                error.0,
                TransportErrorEnum::Io {
                    kind: std::io::ErrorKind::NotFound,
                    ..
                }
            ),
            "{error:?}"
        );
        Ok(())
    })
}
