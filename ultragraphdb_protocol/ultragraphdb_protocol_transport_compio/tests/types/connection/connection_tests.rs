/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::ready::ready;
use compio::io::AsyncWrite;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{BufferPool, NetworkFaults, SimNetwork, TransportError};
use ultragraphdb_protocol_transport_compio::{CompioSimStream, Connection};

use crate::utils::sim::connect;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, Header, Hello, MAX_FRAME_SIZE, Welcome,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fn pair(
    faults: NetworkFaults,
) -> (
    SimNetwork,
    Connection<CompioSimStream>,
    Connection<CompioSimStream>,
) {
    let network = SimNetwork::new(11, faults);
    let (client, server) = connect(&network);
    (
        network,
        Connection::new(client, CLUSTER),
        Connection::new(server, CLUSTER),
    )
}

fn credit(stream: u64) -> Vec<u8> {
    Credit::new(1)
        .frame(Header::new(CLUSTER, Command::Credit).with_stream(stream))
        .unwrap()
}

/// An Error frame of a 6,000-byte message: a body of two chunks and a table.
fn large() -> Vec<u8> {
    ErrorReply::new(ErrorCode::Refused, "m".repeat(6_000))
        .frame(Header::new(CLUSTER, Command::Error).with_stream(3))
        .unwrap()
}

fn raw(connection: &mut Connection<CompioSimStream>, bytes: &[u8]) {
    let compio::BufResult(written, _) = ready(connection.stream_mut().write(bytes.to_vec()));
    written.unwrap();
}

#[test]
fn test_frames_split_in_flight_are_read_whole() {
    let (_network, mut client, mut server) = pair(
        NetworkFaults::none()
            .with_split(1_000_000)
            .with_delay(500_000, 3),
    );
    assert_eq!(client.cluster(), CLUSTER);
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let frames = vec![
        credit(1),
        large(),
        Hello::new(1, 1, 0, "c")
            .frame(Header::new(CLUSTER, Command::Hello))
            .unwrap(),
    ];
    for frame in &frames {
        ready(client.write_frame(frame.clone())).unwrap();
    }
    for frame in &frames {
        let received = ready(server.read_frame(&pool)).unwrap();
        assert_eq!(received.bytes(), &frame[..]);
        assert!(received.view().is_ok());
    }
}

#[test]
fn test_a_close_between_frames_is_closed_and_inside_one_is_truncated() {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    ready(client.write_frame(credit(1))).unwrap();
    ready(client.stream_mut().shutdown()).unwrap();
    assert!(ready(server.read_frame(&pool)).is_ok());
    assert_eq!(
        ready(server.read_frame(&pool)).unwrap_err(),
        TransportError::Closed()
    );
    // Inside the header.
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    raw(&mut client, &large()[..100]);
    ready(client.stream_mut().shutdown()).unwrap();
    assert_eq!(
        ready(server.read_frame(&pool)).unwrap_err(),
        TransportError::Truncated(100, 256)
    );
    // Inside the body.
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let frame = large();
    raw(&mut client, &frame[..4_000]);
    ready(client.stream_mut().shutdown()).unwrap();
    assert_eq!(
        ready(server.read_frame(&pool)).unwrap_err(),
        TransportError::Truncated(4_000, frame.len())
    );
    assert_eq!(pool.available(), 4, "every buffer is back");
}

#[test]
fn test_a_header_breaking_a_rule_is_refused_before_its_body_is_read() {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let other = ErrorReply::new(ErrorCode::Busy, "b".repeat(100))
        .frame(Header::new(7, Command::Error))
        .unwrap();
    ready(client.write_frame(other)).unwrap();
    assert_eq!(
        ready(server.read_frame(&pool)).unwrap_err(),
        TransportError::Protocol(ProtocolError::WrongCluster(CLUSTER, 7))
    );
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let mut broken = credit(1);
    broken[0] ^= 1;
    ready(client.write_frame(broken)).unwrap();
    assert!(matches!(
        ready(server.read_frame(&pool)).unwrap_err().0,
        ultragraphdb_protocol_transport::TransportErrorEnum::Protocol { .. }
    ));
}

#[test]
fn test_the_body_of_a_refused_header_stays_unread() {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let network = SimNetwork::new(3, NetworkFaults::none());
    let (client, server) = connect(&network);
    let probe = server.clone();
    let mut client = Connection::new(client, CLUSTER);
    let mut server = Connection::new(server, CLUSTER);
    let other = ErrorReply::new(ErrorCode::Busy, "b".repeat(100))
        .frame(Header::new(7, Command::Error))
        .unwrap();
    let body = other.len() - 256;
    ready(client.write_frame(other)).unwrap();
    network.run_until_idle();
    assert!(ready(server.read_frame(&pool)).is_err());
    assert_eq!(probe.sim().pending(), body);
}

#[test]
fn test_no_free_buffer_or_a_frame_past_its_capacity_is_refused() {
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let one = BufferPool::new(1, MAX_FRAME_SIZE);
    ready(client.write_frame(credit(1))).unwrap();
    ready(client.write_frame(credit(2))).unwrap();
    let held = ready(server.read_frame(&one)).unwrap();
    assert_eq!(
        ready(server.read_frame(&one)).unwrap_err(),
        TransportError::PoolExhausted(1)
    );
    drop(held);
    assert_eq!(ready(server.read_frame(&one)).unwrap().stream(), 2);
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let small = BufferPool::new(1, 300);
    let frame = large();
    let size = frame.len();
    ready(client.write_frame(frame)).unwrap();
    assert_eq!(
        ready(server.read_frame(&small)).unwrap_err(),
        TransportError::Protocol(ProtocolError::FrameTooLarge(size, 300))
    );
    assert_eq!(small.available(), 1);
}

#[test]
fn test_a_read_of_a_waiting_stream_takes_what_arrives_later() {
    let pool = BufferPool::new(2, MAX_FRAME_SIZE);
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let (written, read) = ready(futures::future::join(
        async { client.write_frame(credit(4)).await },
        server.read_frame(&pool),
    ));
    written.unwrap();
    assert_eq!(read.unwrap().stream(), 4);
}

fn handshake(
    hello: Hello,
    welcome: Welcome,
) -> (
    Result<Welcome, TransportError>,
    Result<Welcome, TransportError>,
) {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let (_n, mut client, mut server) = pair(NetworkFaults::none().with_split(300_000));
    ready(futures::future::join(
        client.hello(&pool, &hello),
        server.welcome(&pool, &welcome),
    ))
}

#[test]
fn test_a_handshake_agrees_on_the_highest_protocol_both_speak() {
    let welcome = Welcome::new(2, 9, 1 << 20, 16, 4, "s");
    let (client, server) = handshake(Hello::new(1, 3, 5, "c"), welcome.clone());
    assert_eq!(client.unwrap(), welcome);
    assert_eq!(server.unwrap(), welcome);
    let (client, _) = handshake(Hello::new(1, 1, 5, "c"), welcome);
    assert_eq!(client.unwrap().protocol(), 1);
}

#[test]
fn test_a_hello_above_the_server_is_refused_on_both_sides() {
    let (client, server) = handshake(
        Hello::new(3, 4, 5, "c"),
        Welcome::new(2, 9, 1 << 20, 16, 4, "s"),
    );
    assert!(matches!(
        client.unwrap_err().0,
        ultragraphdb_protocol_transport::TransportErrorEnum::Refused {
            code: ErrorCode::ProtocolVersion,
            ..
        }
    ));
    assert!(matches!(
        server.unwrap_err().0,
        ultragraphdb_protocol_transport::TransportErrorEnum::Refused {
            code: ErrorCode::ProtocolVersion,
            ..
        }
    ));
}

#[test]
fn test_a_handshake_of_another_command_is_refused() {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    // The client opens with Credit: the server refuses and answers with a protocol error.
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    ready(client.write_frame(credit(1))).unwrap();
    assert_eq!(
        ready(server.welcome(&pool, &Welcome::new(1, 0, 1 << 20, 1, 1, "s"))).unwrap_err(),
        TransportError::UnexpectedCommand(1, 14)
    );
    let answer = ready(client.read_frame(&pool)).unwrap();
    assert_eq!(answer.command(), 16);
    // The server answers Hello with Credit: the client refuses it.
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let (refused, _) = ready(futures::future::join(
        client.hello(&pool, &Hello::new(1, 1, 0, "c")),
        async {
            let _hello = server.read_frame(&pool).await.unwrap();
            server.write_frame(credit(0)).await
        },
    ));
    assert_eq!(
        refused.unwrap_err(),
        TransportError::UnexpectedCommand(2, 14)
    );
    // The server welcomes at a protocol the client did not offer.
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let (refused, _) = ready(futures::future::join(
        client.hello(&pool, &Hello::new(1, 2, 0, "c")),
        async {
            let _hello = server.read_frame(&pool).await.unwrap();
            let welcome = Welcome::new(5, 0, 1 << 20, 1, 1, "s")
                .frame(Header::new(CLUSTER, Command::Welcome))
                .unwrap();
            server.write_frame(welcome).await
        },
    ));
    assert_eq!(
        refused.unwrap_err(),
        TransportError::Protocol(ProtocolError::FieldValue(2, "protocol", 5))
    );
}

#[test]
fn test_a_connection_runs_over_compio_tcp() {
    use compio::net::{TcpListener, TcpStream};
    let runtime = compio::runtime::Runtime::new().expect("a compio runtime");
    runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a listener on loopback");
        let address = listener.local_addr().unwrap();
        let (accepted, connected) =
            futures::future::join(listener.accept(), TcpStream::connect(address)).await;
        let mut server = Connection::new(accepted.unwrap().0, CLUSTER);
        let mut client = Connection::new(connected.unwrap(), CLUSTER);
        let pool = BufferPool::new(4, MAX_FRAME_SIZE);
        let welcome = Welcome::new(1, 0, 1 << 20, 8, 2, "s");
        let (client_side, server_side) = futures::future::join(
            client.hello(&pool, &Hello::new(1, 1, 0, "c")),
            server.welcome(&pool, &welcome),
        )
        .await;
        assert_eq!(client_side.unwrap(), welcome);
        assert_eq!(server_side.unwrap(), welcome);
        let frame = large();
        client.write_frame(frame.clone()).await.unwrap();
        let received = server.read_frame(&pool).await.unwrap();
        assert_eq!(received.bytes(), &frame[..]);
        drop(client);
        assert_eq!(
            server.read_frame(&pool).await.unwrap_err(),
            TransportError::Closed()
        );
    });
}
