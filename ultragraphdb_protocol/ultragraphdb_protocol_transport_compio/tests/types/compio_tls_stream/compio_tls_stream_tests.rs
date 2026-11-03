/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::AsyncWrite;
use rustls::ProtocolVersion;
use ultragraphdb_protocol_transport::{Address, BufferPool, Framer};
use ultragraphdb_protocol_transport_compio::{Carrier, CompioTlsStream, Connection, Listener};
use ultragraphdb_protocol_wire::{Hello, MAX_FRAME_SIZE, Welcome};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{TestPki, TlsErrorEnum};

use crate::utils::bytes::{read_once, receive, send};
use crate::utils::socket::SocketDir;

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A server's and a client's TLS stream over carriers of `address`, their handshake completed.
async fn tls_pair(
    pki: &TestPki,
    address: &Address,
) -> TestResult<(CompioTlsStream<Carrier>, CompioTlsStream<Carrier>)> {
    let listener = Listener::bind(address).await?;
    let bound = listener.local_address()?;
    let (accepted, dialled) =
        futures::future::join(listener.accept(), Carrier::connect(&bound)).await;
    let (acceptor, connector) = (pki.acceptor()?, pki.connector()?);
    let (server, client) = futures::future::join(
        CompioTlsStream::accept(&acceptor, accepted?),
        CompioTlsStream::connect(&connector, dialled?, "localhost"),
    )
    .await;
    Ok((server?, client?))
}

#[test]
fn test_a_line_crosses_tls_over_tcp_and_over_a_unix_socket() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let pki = TestPki::new()?;
        for address in [Address::parse("tcp://127.0.0.1:0")?, dir.address("s.sock")?] {
            let (mut server, mut client) = tls_pair(&pki, &address).await?;
            assert_eq!(
                client.session().protocol_version(),
                Some(ProtocolVersion::TLSv1_3)
            );
            assert_eq!(
                server.session().protocol_version(),
                Some(ProtocolVersion::TLSv1_3)
            );
            assert_eq!(
                (client.session().host(), server.session().host()),
                ("localhost", "")
            );
            send(&mut client, b"ping\n").await?;
            let line = receive(&mut server, 5).await?;
            assert_eq!(line, b"ping\n", "{address}");
            send(&mut server, &line).await?;
            assert_eq!(receive(&mut client, 5).await?, b"ping\n", "{address}");
            // The client's close_notify reads as the end on the server.
            client.shutdown().await.context("a shutdown")?;
            assert_eq!(read_once(&mut server).await?, 0, "{address}");
            assert!(matches!(
                server.get_ref(),
                Carrier::Tcp(_) | Carrier::Unix(_)
            ));
        }
        Ok::<(), TestError>(())
    })
}

#[test]
fn test_a_connection_runs_its_handshake_inside_tls_over_a_unix_socket() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let pki = TestPki::new()?;
        let (server, client) = tls_pair(&pki, &dir.address("s.sock")?).await?;
        let mut server = Connection::new(server, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
        let mut client = Connection::new(client, Framer::new(CLUSTER, MAX_FRAME_SIZE));
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
fn test_a_server_of_an_untrusted_ca_fails_both_handshakes() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let (pki, other) = (TestPki::new()?, TestPki::new()?);
        let listener = Listener::bind(&Address::parse("tcp://127.0.0.1:0")?).await?;
        let bound = listener.local_address()?;
        let (accepted, dialled) =
            futures::future::join(listener.accept(), Carrier::connect(&bound)).await;
        let (acceptor, connector) = (pki.acceptor()?, other.connector()?);
        let (server, client) = futures::future::join(
            CompioTlsStream::accept(&acceptor, accepted?),
            CompioTlsStream::connect(&connector, dialled?, "localhost"),
        )
        .await;
        let Err(error) = client else {
            return Err(TestError::new("another CA's certificate verified"));
        };
        assert!(
            matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
                if host == "localhost" && reason.contains("UnknownIssuer")),
            "{error:?}"
        );
        assert!(
            server.is_err(),
            "the server took the client's alert as a failed handshake"
        );
        Ok::<(), TestError>(())
    })
}

#[test]
fn test_a_client_that_names_no_server_is_refused_before_dialling_bytes() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let pki = TestPki::new()?;
        let listener = Listener::bind(&Address::parse("tcp://127.0.0.1:0")?).await?;
        let dialled = Carrier::connect(&listener.local_address()?).await?;
        let Err(error) = CompioTlsStream::connect(&pki.connector()?, dialled, "not a host").await
        else {
            return Err(TestError::new("\"not a host\" was taken as a server name"));
        };
        assert_eq!(
            error.0,
            TlsErrorEnum::InvalidServerName("not a host".to_string())
        );
        Ok::<(), TestError>(())
    })
}
