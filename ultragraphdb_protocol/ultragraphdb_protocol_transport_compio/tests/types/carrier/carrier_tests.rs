/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::AsyncWrite;
use compio::io::util::Splittable;
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

/// Bytes cross both ways between two carriers each split into halves: what one's write half sends, the other's read
/// half reads.
async fn split_round_trip(near: Carrier, far: Carrier) -> TestResult {
    let (mut near_read, mut near_write) = near.split();
    let (mut far_read, mut far_write) = far.split();
    send(&mut near_write, b"ping").await?;
    assert_eq!(receive(&mut far_read, 4).await?, b"ping");
    send(&mut far_write, b"pong").await?;
    assert_eq!(receive(&mut near_read, 4).await?, b"pong");
    Ok(())
}

#[test]
fn test_a_split_carrier_carries_bytes_both_ways_over_tcp_and_unix() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let (_tcp, tcp_accepted, tcp_dialled) = pair(&Address::any_port("127.0.0.1")?).await?;
        split_round_trip(tcp_dialled, tcp_accepted).await?;
        let dir = SocketDir::new()?;
        let (_unix, unix_accepted, unix_dialled) = pair(&dir.address("s.sock")?).await?;
        split_round_trip(unix_dialled, unix_accepted).await
    })
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
        let hello = Hello::new(1, 1, 0, u32::try_from(MAX_FRAME_SIZE)?, "c");
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

/// Fills `stream`'s send buffer: writes until the socket refuses more.
fn fill(stream: &mut std::os::unix::net::UnixStream) -> TestResult {
    use std::io::Write;
    stream.set_nonblocking(true)?;
    let block = [0u8; 4096];
    // A loop bounded by the socket's buffer, a few hundred KiB: each pass writes or ends it.
    loop {
        match stream.write(&block) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

/// Reads everything `stream` holds, so its peer's send buffer has room again.
fn drain(stream: &mut std::os::unix::net::UnixStream) -> TestResult {
    use std::io::Read;
    stream.set_nonblocking(true)?;
    let mut block = [0u8; 4096];
    // A loop bounded by what the peer wrote: each pass reads or ends it.
    loop {
        match stream.read(&mut block) {
            Ok(0) => return Err(TestError::new("the peer closed while it was drained")),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

/// `future`'s output, or a [`TestError`] naming `what` when it is not ready within a second: it was never woken.
async fn within<F: std::future::Future>(future: F, what: &str) -> TestResult<F::Output> {
    compio::time::timeout(std::time::Duration::from_secs(1), future)
        .await
        .map_err(|_| TestError::new(format!("{what} was never woken within 1 s")))
}

/// A read waiting on one carrier takes its byte when, in the same wake-up, another carrier's write and read become ready
/// together and its write is done first. On the polling driver over kqueue a socket's two waits share the write's key, so
/// the read's event finds the write done; compio-driver 0.12.5 then left the whole batch, losing every later event of it,
/// and a read whose event was lost waited forever (compio-rs/compio#1119): the system link's stall under frames of a
/// megabyte, the group's read of a Declare never taken.
#[test]
fn test_a_read_waking_beside_a_split_carriers_write_and_read_takes_its_byte() -> TestResult {
    use compio::io::AsyncRead;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::pin::pin;
    use std::task::Poll;
    compio::runtime::Runtime::new()?.block_on(async {
        let (mut both, mut both_peer) = UnixStream::pair()?;
        fill(&mut both)?;
        let (mut both_read, mut both_write) =
            Carrier::Unix(compio::net::UnixStream::from_std(both)?).split();
        let (other, mut other_peer) = UnixStream::pair()?;
        let mut other = Carrier::Unix(compio::net::UnixStream::from_std(other)?);
        let mut write = pin!(both_write.write(vec![1u8]));
        let mut read = pin!(both_read.read(Vec::with_capacity(1)));
        let mut other_read = pin!(other.read(Vec::with_capacity(1)));
        // Polled once each, the three wait in the driver: the full socket's write and its read, and the other's read.
        let waiting = std::future::poll_fn(|cx| {
            Poll::Ready([
                write.as_mut().poll(cx).is_pending(),
                read.as_mut().poll(cx).is_pending(),
                other_read.as_mut().poll(cx).is_pending(),
            ])
        })
        .await;
        assert_eq!(waiting, [true; 3], "write, read and the other read wait");
        // Before the driver looks again, all three become ready, in this order: room for the write, a byte for its read,
        // a byte for the other read. kqueue reports them in one batch, in the order they became ready.
        drain(&mut both_peer)?;
        both_peer.write_all(b"r")?;
        other_peer.write_all(b"o")?;
        let compio::BufResult(written, _) = within(write, "the write").await?;
        assert_eq!(written?, 1, "the write");
        let compio::BufResult(beside, _) = within(read, "the read beside it").await?;
        assert_eq!(beside?, 1, "the read beside it");
        let compio::BufResult(other, got) = within(other_read, "the other carrier's read").await?;
        assert_eq!(other?, 1, "the other carrier's read");
        assert_eq!(got, b"o");
        Ok::<(), TestError>(())
    })
}

/// A TCP carrier knows its peer's address; a Unix-domain carrier names no peer address and has neither keepalive nor
/// `TCP_NODELAY` to report.
#[test]
fn test_only_a_tcp_carrier_names_its_peer_and_holds_tcp_options() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let (_listener, accepted, dialled) = pair(&Address::any_port("127.0.0.1")?).await?;
        let loopback = Some(std::net::IpAddr::from([127, 0, 0, 1]));
        assert_eq!(accepted.peer_ip(), loopback, "accepted");
        assert_eq!(dialled.peer_ip(), loopback, "dialled");
        let dir = SocketDir::new()?;
        let (_unix_listener, unix, _dialled) = pair(&dir.address("s.sock")?).await?;
        assert_eq!(unix.peer_ip(), None);
        assert!(!unix.is_kept_alive()?, "no keepalive");
        assert!(!unix.is_no_delay()?, "no TCP_NODELAY");
        Ok(())
    })
}
