/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::ready::ready;
use compio::io::AsyncWrite;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    BufferPool, Framer, NetworkFaults, SimNetwork, TransportError,
};
use ultragraphdb_protocol_transport_compio::{CompioSimStream, Connection};

use crate::utils::sim::connect;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, Header, Hello, MAX_FRAME_SIZE, Welcome,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

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
        Connection::new(client, Framer::new(CLUSTER, MAX_FRAME_SIZE)),
        Connection::new(server, Framer::from_client(CLUSTER, MAX_FRAME_SIZE)),
    )
}

fn credit(stream: u64) -> TestResult<Vec<u8>> {
    Ok(Credit::new(1).frame(Header::new(CLUSTER, Command::Credit).with_stream(stream))?)
}

/// An Error frame of a 6,000-byte message: a body of two chunks and a table.
fn large() -> TestResult<Vec<u8>> {
    Ok(ErrorReply::new(ErrorCode::Refused, "m".repeat(6_000))
        .frame(Header::new(CLUSTER, Command::Error).with_stream(3))?)
}

/// `frame` numbered as its sender's first, sequence 1, as a handshake's frame is.
fn first(mut frame: Vec<u8>) -> TestResult<Vec<u8>> {
    Header::stamp_sequence(&mut frame, 1)?;
    Ok(frame)
}

/// The error `result` failed with; a [`TestError`] naming `what` when it succeeded instead.
fn failure<T>(result: Result<T, TransportError>, what: &str) -> TestResult<TransportError> {
    let Err(error) = result else {
        return Err(TestError::new(format!(
            "{what}: succeeded, a failure was expected"
        )));
    };
    Ok(error)
}

fn raw(connection: &mut Connection<CompioSimStream>, bytes: &[u8]) -> TestResult {
    let compio::BufResult(written, _) = ready(connection.stream_mut().write(bytes.to_vec()));
    assert_eq!(written?, bytes.len());
    Ok(())
}

#[test]
fn test_frames_split_in_flight_are_read_whole() -> TestResult {
    let (_network, mut client, mut server) = pair(
        NetworkFaults::none()
            .with_split(1_000_000)
            .with_delay(500_000, 3),
    );
    assert_eq!(client.cluster(), CLUSTER);
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let unnumbered = vec![
        credit(1)?,
        large()?,
        Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c")
            .frame(Header::new(CLUSTER, Command::Hello))?,
    ];
    // Numbered as a client numbers them, from 1, so each view holds the connection's sequence.
    let frames = unnumbered
        .into_iter()
        .zip(1u128..)
        .map(|(mut frame, sequence)| {
            Header::stamp_sequence(&mut frame, sequence)?;
            Ok(frame)
        })
        .collect::<TestResult<Vec<Vec<u8>>>>()?;
    for frame in &frames {
        assert_eq!(ready(client.write_frame(frame.clone()))?, *frame);
    }
    for (frame, command) in frames
        .iter()
        .zip([Command::Credit, Command::Error, Command::Hello])
    {
        let received = ready(server.read_frame(&pool))?;
        assert_eq!(received.bytes(), &frame[..]);
        assert_eq!(received.view()?.command(), command);
    }
    Ok(())
}

#[test]
fn test_a_close_between_frames_is_closed_and_inside_one_is_truncated() -> TestResult {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        assert_eq!(ready(client.write_frame(credit(1)?))?, credit(1)?);
        ready(client.stream_mut().shutdown())?;
        assert_eq!(ready(server.read_frame(&pool))?.stream(), 1);
        assert_eq!(
            failure(ready(server.read_frame(&pool)), "a read after the close")?,
            TransportError::closed()
        );
    }
    // Inside the header.
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        raw(&mut client, &large()?[..100])?;
        ready(client.stream_mut().shutdown())?;
        assert_eq!(
            failure(ready(server.read_frame(&pool)), "a read of a cut header")?,
            TransportError::truncated(100, 256)
        );
    }
    // Inside the body.
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let frame = large()?;
        raw(&mut client, &frame[..4_000])?;
        ready(client.stream_mut().shutdown())?;
        assert_eq!(
            failure(ready(server.read_frame(&pool)), "a read of a cut body")?,
            TransportError::truncated(4_000, frame.len())
        );
    }
    assert_eq!(pool.available(), 4, "every buffer is back");
    Ok(())
}

#[test]
fn test_a_header_breaking_a_rule_is_refused_before_its_body_is_read() -> TestResult {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let other = ErrorReply::new(ErrorCode::Busy, "b".repeat(100))
            .frame(Header::new(7, Command::Error))?;
        assert_eq!(ready(client.write_frame(other.clone()))?, other);
        assert_eq!(
            failure(
                ready(server.read_frame(&pool)),
                "a read of another cluster's frame"
            )?,
            TransportError::protocol(ProtocolError::wrong_cluster(CLUSTER, 7))
        );
    }
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let mut broken = credit(1)?;
        // Bytes 0 to 7 are the header's checksum (PROTOCOL.md section 2.5, step 2), which the framer checks first:
        // the stored one is off by its lowest bit, the computed one is the frame's own.
        let mut checksum = [0u8; 8];
        checksum.copy_from_slice(&broken[..8]);
        let computed = u64::from_le_bytes(checksum);
        broken[0] ^= 1;
        assert_eq!(ready(client.write_frame(broken.clone()))?, broken);
        assert_eq!(
            failure(ready(server.read_frame(&pool)), "a read of a broken header")?,
            TransportError::protocol(ProtocolError::header_checksum(computed ^ 1, computed))
        );
    }
    Ok(())
}

#[test]
fn test_the_body_of_a_refused_header_stays_unread() -> TestResult {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let network = SimNetwork::new(3, NetworkFaults::none());
    let (client_end, server_end) = connect(&network);
    let probe = server_end.clone();
    let mut client = Connection::new(client_end, Framer::new(CLUSTER, MAX_FRAME_SIZE));
    let mut server = Connection::new(server_end, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
    let other =
        ErrorReply::new(ErrorCode::Busy, "b".repeat(100)).frame(Header::new(7, Command::Error))?;
    let body = other.len() - 256;
    assert_eq!(ready(client.write_frame(other.clone()))?, other);
    network.run_until_idle();
    assert_eq!(
        failure(
            ready(server.read_frame(&pool)),
            "a read of another cluster's frame"
        )?,
        TransportError::protocol(ProtocolError::wrong_cluster(CLUSTER, 7))
    );
    assert_eq!(probe.sim().pending(), body);
    Ok(())
}

/// A full pool is backpressure: a read with no buffer free waits, and the buffer given back takes the next frame.
#[test]
fn test_no_free_buffer_waits_and_a_frame_past_its_capacity_is_refused() -> TestResult {
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let one = BufferPool::new(1, MAX_FRAME_SIZE);
        assert_eq!(ready(client.write_frame(credit(1)?))?, credit(1)?);
        assert_eq!(ready(client.write_frame(credit(2)?))?, credit(2)?);
        let held = ready(server.read_frame(&one))?;
        let mut waiting = std::pin::pin!(server.read_frame(&one));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(
            waiting.as_mut().poll(&mut cx).is_pending(),
            "a read with no buffer free waits"
        );
        drop(held);
        assert_eq!(ready(waiting)?.stream(), 2);
    }
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let small = BufferPool::new(1, 300);
        let frame = large()?;
        let size = frame.len();
        assert_eq!(ready(client.write_frame(frame.clone()))?, frame);
        assert_eq!(
            failure(
                ready(server.read_frame(&small)),
                "a read of a frame past the buffers"
            )?,
            TransportError::protocol(ProtocolError::frame_too_large(size, 300))
        );
        assert_eq!(small.available(), 1);
    }
    Ok(())
}

#[test]
fn test_a_read_of_a_waiting_stream_takes_what_arrives_later() -> TestResult {
    let pool = BufferPool::new(2, MAX_FRAME_SIZE);
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let frame = credit(4)?;
    let (written, read) = ready(futures::future::join(
        async { client.write_frame(frame.clone()).await },
        server.read_frame(&pool),
    ));
    assert_eq!(written?, frame);
    assert_eq!(read?.stream(), 4);
    Ok(())
}

fn handshake(
    hello: &Hello,
    welcome: &Welcome,
) -> (
    Result<Welcome, TransportError>,
    Result<Welcome, TransportError>,
) {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let (_n, mut client, mut server) = pair(NetworkFaults::none().with_split(300_000));
    ready(futures::future::join(
        client.hello(&pool, hello),
        server.welcome(&pool, welcome),
    ))
}

#[test]
fn test_a_handshake_agrees_on_the_highest_protocol_both_speak() -> TestResult {
    let welcome = Welcome::new(2, 9, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(16)
        .with_initial_credit(4);
    let (client, server) = handshake(&Hello::new(1, 3, 5, MAX_FRAME_SIZE as u32, "c"), &welcome);
    assert_eq!(client?, welcome);
    assert_eq!(server?, welcome);
    let (lower_client, _) = handshake(&Hello::new(1, 1, 5, MAX_FRAME_SIZE as u32, "c"), &welcome);
    assert_eq!(lower_client?.protocol(), 1);
    Ok(())
}

#[test]
fn test_a_hello_above_the_server_is_refused_on_both_sides() -> TestResult {
    let (client, server) = handshake(
        &Hello::new(3, 4, 5, MAX_FRAME_SIZE as u32, "c"),
        &Welcome::new(2, 9, "s")
            .with_max_frame(1 << 20)
            .with_max_streams(16)
            .with_initial_credit(4),
    );
    let (client, server) = (
        failure(client, "the client's handshake")?,
        failure(server, "the server's handshake")?,
    );
    assert!(matches!(
        &client.0,
        ultragraphdb_protocol_transport::TransportErrorEnum::Refused {
            code: ErrorCode::ProtocolVersion,
            message,
        } if !message.is_empty()
    ));
    // The client reads the very refusal the server returned: its code and its message.
    assert_eq!(client, server);
    Ok(())
}

#[test]
fn test_a_handshake_of_another_command_is_refused() -> TestResult {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    // The client opens with Credit: the server refuses and answers with a protocol error.
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let opening = first(credit(1)?)?;
        assert_eq!(ready(client.write_frame(opening.clone()))?, opening);
        assert_eq!(
            failure(
                ready(
                    server.welcome(
                        &pool,
                        &Welcome::new(1, 0, "s")
                            .with_max_frame(1 << 20)
                            .with_max_streams(1)
                            .with_initial_credit(1)
                    )
                ),
                "the server's handshake of a Credit"
            )?,
            TransportError::unexpected_command(1, 14)
        );
        let answer = ready(client.read_frame(&pool))?;
        assert_eq!(answer.command(), 16);
    }
    // The server answers Hello with Credit: the client refuses it.
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let (refused, answered) = ready(futures::future::join(
            client.hello(&pool, &Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c")),
            async {
                let _hello = server.read_frame(&pool).await?;
                Ok::<Vec<u8>, TestError>(server.write_frame(first(credit(0)?)?).await?)
            },
        ));
        assert_eq!(
            failure(refused, "the client's handshake answered by a Credit")?,
            TransportError::unexpected_command(2, 14)
        );
        let _answer: Vec<u8> = answered?;
    }
    // The server welcomes at a protocol the client did not offer.
    {
        let (_n, mut client, mut server) = pair(NetworkFaults::none());
        let (refused, answered) = ready(futures::future::join(
            client.hello(&pool, &Hello::new(1, 2, 0, MAX_FRAME_SIZE as u32, "c")),
            async {
                let _hello = server.read_frame(&pool).await?;
                let welcome = Welcome::new(5, 0, "s")
                    .with_max_frame(1 << 20)
                    .with_max_streams(1)
                    .with_initial_credit(1)
                    .frame(Header::new(CLUSTER, Command::Welcome).with_sequence(1))?;
                Ok::<Vec<u8>, TestError>(server.write_frame(welcome).await?)
            },
        ));
        assert_eq!(
            failure(refused, "the client's handshake welcomed at protocol 5")?,
            TransportError::protocol(ProtocolError::field_value(2, "protocol", 5))
        );
        let _answer: Vec<u8> = answered?;
    }
    Ok(())
}

#[test]
fn test_a_connection_runs_over_compio_tcp() -> TestResult {
    use compio::net::{TcpListener, TcpStream};
    let runtime = compio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (accepted, connected) =
            futures::future::join(listener.accept(), TcpStream::connect(address)).await;
        let mut server = Connection::new(accepted?.0, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
        let mut client = Connection::new(connected?, Framer::new(CLUSTER, MAX_FRAME_SIZE));
        let pool = BufferPool::new(4, MAX_FRAME_SIZE);
        let welcome = Welcome::new(1, 0, "s")
            .with_max_frame(1 << 20)
            .with_max_streams(8)
            .with_initial_credit(2);
        let (client_side, server_side) = futures::future::join(
            client.hello(&pool, &Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c")),
            server.welcome(&pool, &welcome),
        )
        .await;
        assert_eq!(client_side?, welcome);
        assert_eq!(server_side?, welcome);
        let frame = large()?;
        assert_eq!(client.write_frame(frame.clone()).await?, frame);
        let received = server.read_frame(&pool).await?;
        assert_eq!(received.bytes(), &frame[..]);
        drop(client);
        assert_eq!(
            failure(
                server.read_frame(&pool).await,
                "a read after the client dropped"
            )?,
            TransportError::closed()
        );
        Ok::<(), TestError>(())
    })
}

/// A stream of the tests' own: its first read is interrupted, as a signal interrupts a system call, then it serves
/// `incoming`; what is written is held until a flush hands it on to `flushed`.
#[derive(Debug, Default)]
struct Interrupting {
    interrupted: bool,
    incoming: Vec<u8>,
    at: usize,
    held: Vec<u8>,
    flushed: Vec<u8>,
}

impl compio::io::AsyncRead for Interrupting {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::BufResult<usize, B> {
        if !self.interrupted {
            self.interrupted = true;
            return compio::BufResult(Err(std::io::ErrorKind::Interrupted.into()), buf);
        }
        let mut source: &[u8] = &self.incoming[self.at..];
        let compio::BufResult(read, buf) = source.read(buf).await;
        if let Ok(n) = read {
            self.at += n;
        }
        compio::BufResult(read, buf)
    }
}

impl AsyncWrite for Interrupting {
    async fn write<T: compio::buf::IoBuf>(&mut self, buf: T) -> compio::BufResult<usize, T> {
        self.held.extend_from_slice(buf.as_init());
        compio::BufResult(Ok(buf.as_init().len()), buf)
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        self.flushed.append(&mut self.held);
        Ok(())
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_an_interrupted_read_is_retried() -> TestResult {
    let stream = Interrupting {
        incoming: credit(4)?,
        ..Interrupting::default()
    };
    let mut connection = Connection::new(stream, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let frame = ready(connection.read_frame(&pool))?;
    assert_eq!(frame.stream(), 4);
    assert!(connection.stream_mut().interrupted);
    Ok(())
}

#[test]
fn test_a_frame_written_is_flushed() -> TestResult {
    let mut connection = Connection::new(
        Interrupting::default(),
        Framer::new(CLUSTER, MAX_FRAME_SIZE),
    );
    assert_eq!(ready(connection.write_frame(credit(1)?))?, credit(1)?);
    let stream = connection.into_inner();
    assert_eq!(stream.flushed, credit(1)?);
    assert!(stream.held.is_empty());
    Ok(())
}

/// A stream of the tests' own whose first read fails with `ConnectionReset`, then serves `incoming`: a failure that is
/// not an interruption ends the read, so the bytes after it are never reached.
#[derive(Debug, Default)]
struct Resetting {
    failed: bool,
    incoming: Vec<u8>,
}

impl compio::io::AsyncRead for Resetting {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::BufResult<usize, B> {
        if !self.failed {
            self.failed = true;
            return compio::BufResult(Err(std::io::ErrorKind::ConnectionReset.into()), buf);
        }
        let mut source: &[u8] = &self.incoming;
        source.read(buf).await
    }
}

impl AsyncWrite for Resetting {
    async fn write<T: compio::buf::IoBuf>(&mut self, buf: T) -> compio::BufResult<usize, T> {
        let len = buf.as_init().len();
        compio::BufResult(Ok(len), buf)
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_a_failed_read_is_an_io_error_and_not_retried() -> TestResult {
    let stream = Resetting {
        incoming: credit(4)?,
        ..Resetting::default()
    };
    let mut connection = Connection::new(stream, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    // The error the stream raised, as `TransportError::io` records it: its kind and its text.
    let expected = TransportError::io(&std::io::ErrorKind::ConnectionReset.into());
    assert_eq!(
        failure(
            ready(connection.read_frame(&pool)),
            "a read of a reset stream"
        )?,
        expected
    );
    assert_eq!(pool.available(), 1, "the buffer is back");
    Ok(())
}

#[test]
fn test_a_shutdown_closes_the_peers_reads() -> TestResult {
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    assert_eq!(ready(client.write_frame(credit(1)?))?, credit(1)?);
    ready(client.shutdown());
    // What was sent before the shutdown is read, then the close between frames.
    assert_eq!(ready(server.read_frame(&pool))?.stream(), 1);
    assert_eq!(
        failure(ready(server.read_frame(&pool)), "a read after the shutdown")?,
        TransportError::closed()
    );
    Ok(())
}

#[test]
fn test_a_pool_smaller_than_a_header_refuses_the_frame_naming_its_capacity() -> TestResult {
    // 100 bytes is below the header's 256: the read still takes the whole header, so the header is verified and its
    // size refused against the pool's capacity, rather than the read stopping at the buffer's end.
    let small = BufferPool::new(1, 100);
    let (_n, mut client, mut server) = pair(NetworkFaults::none());
    let frame = credit(1)?;
    let size = frame.len();
    assert_eq!(ready(client.write_frame(frame.clone()))?, frame);
    assert_eq!(
        failure(
            ready(server.read_frame(&small)),
            "a read into a buffer below a header"
        )?,
        TransportError::protocol(ProtocolError::frame_too_large(size, 100))
    );
    assert_eq!(small.available(), 1);
    Ok(())
}
