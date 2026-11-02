/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, HEADER_SIZE, Header, Hello, MAX_FRAME_SIZE,
};

use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::feed::feed;

const CLUSTER: u64 = 7;

fn hello() -> TestResult<Vec<u8>> {
    Ok(Hello::new(
        1,
        1,
        3,
        MAX_FRAME_SIZE as u32,
        "a client named at some length",
    )
    .frame(Header::new(CLUSTER, Command::Hello))?)
}

fn credit() -> TestResult<Vec<u8>> {
    Ok(Credit::new(1).frame(Header::new(CLUSTER, Command::Credit).with_stream(3))?)
}

#[test]
fn test_the_framer_asks_for_the_header_then_the_frames_size() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let frame = hello()?;
    let mut framer = Framer::new(CLUSTER, MAX_FRAME_SIZE);
    assert_eq!(framer.want(), HEADER_SIZE);
    assert!(framer.between_frames());
    let mut buffer = pool.acquire()?;
    let mut header_start = buffer.take();
    header_start.extend_from_slice(&frame[..100]);
    buffer.restore(header_start);
    assert_eq!(
        framer.advance(&buffer),
        Ok(FrameStep::Read(HEADER_SIZE)),
        "short: read on"
    );
    let mut header = buffer.take();
    header.extend_from_slice(&frame[100..HEADER_SIZE]);
    buffer.restore(header);
    assert_eq!(framer.advance(&buffer), Ok(FrameStep::Read(frame.len())));
    assert_eq!(framer.want(), frame.len());
    assert!(!framer.between_frames());
    let mut whole = buffer.take();
    whole.extend_from_slice(&frame[HEADER_SIZE..]);
    buffer.restore(whole);
    assert_eq!(framer.advance(&buffer), Ok(FrameStep::Frame));
    let received = framer.finish(buffer);
    assert_eq!(received.bytes(), &frame[..]);
    assert_eq!(
        framer.want(),
        HEADER_SIZE,
        "the next frame starts at its header"
    );
    Ok(())
}

#[test]
fn test_a_frame_of_a_header_alone_is_whole_at_its_header() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let frame = credit()?;
    assert_eq!(frame.len(), HEADER_SIZE);
    let received = feed(
        &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
        &pool,
        &frame,
        &mut 0,
        1,
    )?;
    assert_eq!(received.bytes(), &frame[..]);
    Ok(())
}

#[test]
fn test_frames_back_to_back_are_read_in_order_whatever_the_reads() -> TestResult {
    let pool = BufferPool::new(4, 4_096);
    let stream = [hello()?, credit()?, hello()?].concat();
    for step in [1, 3, 256, 4_096] {
        let (mut framer, mut at) = (Framer::new(CLUSTER, MAX_FRAME_SIZE), 0);
        let sizes = (0..3)
            .map(|_| {
                Ok(feed(&mut framer, &pool, &stream, &mut at, step)?
                    .bytes()
                    .len())
            })
            .collect::<TestResult<Vec<usize>>>()?;
        assert_eq!(
            sizes,
            vec![hello()?.len(), HEADER_SIZE, hello()?.len()],
            "step {step}"
        );
        assert_eq!(at, stream.len());
    }
    Ok(())
}

#[test]
fn test_a_header_is_refused_before_its_body_is_read() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let mut frame = hello()?;
    frame[60] ^= 1;
    let mut at = 0;
    let refused = feed(
        &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
        &pool,
        &frame,
        &mut at,
        1 << 20,
    )
    .err()
    .context("the refusal")?;
    // Byte 60 lies in the header's `stream`, which the checksum of bytes 8 to 255 covers (PROTOCOL.md section 2.5,
    // step 2).
    assert!(
        matches!(
            refused.0,
            TransportErrorEnum::Protocol {
                error: ProtocolError(ProtocolErrorEnum::HeaderChecksum { .. })
            }
        ),
        "{refused:?}"
    );
    assert_eq!(at, HEADER_SIZE, "no byte of the body was asked for");
    let other = feed(
        &mut Framer::new(CLUSTER + 1, MAX_FRAME_SIZE),
        &pool,
        &hello()?,
        &mut 0,
        1 << 20,
    )
    .err()
    .context("the refusal")?;
    // Step 5: the framer's cluster expected, the frame's found.
    assert_eq!(
        other,
        TransportError::protocol(ProtocolError::wrong_cluster(CLUSTER + 1, CLUSTER))
    );
    Ok(())
}

#[test]
fn test_a_frame_larger_than_the_buffer_is_refused_naming_its_capacity() -> TestResult {
    let pool = BufferPool::new(1, 300);
    let frame = ErrorReply::new(ErrorCode::Busy, "x".repeat(100))
        .frame(Header::new(CLUSTER, Command::Error).with_stream(1))?;
    let refused = feed(
        &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
        &pool,
        &frame,
        &mut 0,
        1 << 20,
    )
    .err()
    .context("the refusal")?;
    assert_eq!(
        refused,
        TransportError::protocol(ProtocolError::frame_too_large(frame.len(), 300))
    );
    // One byte short of the frame: refused, naming the capacity.
    let short = BufferPool::new(1, frame.len() - 1);
    assert_eq!(
        feed(
            &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
            &short,
            &frame,
            &mut 0,
            1 << 20
        )
        .err()
        .context("the refusal")?,
        TransportError::protocol(ProtocolError::frame_too_large(frame.len(), frame.len() - 1))
    );
    let fits = BufferPool::new(1, frame.len());
    let received = feed(
        &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
        &fits,
        &frame,
        &mut 0,
        1 << 20,
    )
    .context("a frame of the capacity fits")?;
    assert_eq!(received.bytes(), &frame[..]);
    Ok(())
}

#[test]
fn test_a_close_between_frames_is_closed_and_inside_one_is_truncated() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let frame = hello()?;
    let closed = |bytes: &[u8]| {
        feed(
            &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
            &pool,
            bytes,
            &mut 0,
            1 << 20,
        )
        .err()
        .context("the refusal")
    };
    assert_eq!(closed(&[])?, TransportError::closed());
    assert_eq!(
        closed(&frame[..3])?,
        TransportError::truncated(3, HEADER_SIZE)
    );
    assert_eq!(
        closed(&frame[..HEADER_SIZE])?,
        TransportError::truncated(HEADER_SIZE, frame.len())
    );
    assert_eq!(
        closed(&frame[..frame.len() - 1])?,
        TransportError::truncated(frame.len() - 1, frame.len())
    );
    let mut framer = Framer::new(CLUSTER, MAX_FRAME_SIZE);
    assert_eq!(
        feed(&mut framer, &pool, &frame[..HEADER_SIZE], &mut 0, 1 << 20)
            .err()
            .context("the refusal")?,
        TransportError::truncated(HEADER_SIZE, frame.len())
    );
    assert!(framer.between_frames(), "a close resets the framer");
    Ok(())
}

#[test]
fn test_a_reset_forgets_a_frame_in_progress() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let frame = hello()?;
    let mut framer = Framer::new(CLUSTER, MAX_FRAME_SIZE);
    let mut buffer = pool.acquire()?;
    let mut bytes = buffer.take();
    bytes.extend_from_slice(&frame[..HEADER_SIZE]);
    buffer.restore(bytes);
    assert_eq!(framer.advance(&buffer), Ok(FrameStep::Read(frame.len())));
    framer.reset();
    assert_eq!(framer.want(), HEADER_SIZE);
    assert!(framer.between_frames());
    assert_eq!(framer.cluster(), CLUSTER);
    Ok(())
}

#[test]
fn test_a_close_with_nothing_of_the_body_read_is_truncated_at_the_frames_size() -> TestResult {
    // A shell whose buffer was emptied after the header reports 0 bytes of a frame in progress: still inside the
    // frame, so truncated at its size, not a close between frames.
    let pool = BufferPool::new(1, 4_096);
    let frame = hello()?;
    let mut framer = Framer::new(CLUSTER, MAX_FRAME_SIZE);
    let mut buffer = pool.acquire()?;
    let mut bytes = buffer.take();
    bytes.extend_from_slice(&frame[..HEADER_SIZE]);
    buffer.restore(bytes);
    assert_eq!(framer.advance(&buffer), Ok(FrameStep::Read(frame.len())));
    assert_eq!(framer.closed(0), TransportError::truncated(0, frame.len()));
    assert!(
        framer.between_frames(),
        "the framer waits for a header again"
    );
    assert_eq!(framer.closed(0), TransportError::closed());
    Ok(())
}

#[test]
fn test_a_header_past_the_announced_maximum_is_refused_at_the_header() -> TestResult {
    let pool = BufferPool::new(3, MAX_FRAME_SIZE);
    // A Write of 600,000 bytes of text: past a framer that announced 524,288 bytes.
    let big = ErrorReply::new(ErrorCode::Refused, "x".repeat(600_000))
        .frame(Header::new(CLUSTER, Command::Error).with_stream(3))?;
    let refused = feed(
        &mut Framer::new(CLUSTER, 524_288),
        &pool,
        &big,
        &mut 0,
        1 << 20,
    )
    .err()
    .context("the refusal")?;
    assert_eq!(
        refused,
        TransportError::protocol(ProtocolError::frame_too_large(big.len(), 524_288))
    );
    let read = feed(
        &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
        &pool,
        &big,
        &mut 0,
        1 << 20,
    )
    .context("within 1 MiB")?;
    assert_eq!(read.bytes(), &big[..]);
    // At the announced maximum exactly the frame is taken; one byte below it, refused naming that maximum.
    let at = feed(
        &mut Framer::new(CLUSTER, big.len()),
        &pool,
        &big,
        &mut 0,
        1 << 20,
    )
    .context("a frame of the announced maximum")?;
    assert_eq!(at.bytes().len(), big.len());
    assert_eq!(
        feed(
            &mut Framer::new(CLUSTER, big.len() - 1),
            &pool,
            &big,
            &mut 0,
            1 << 20
        )
        .err()
        .context("the refusal")?,
        TransportError::protocol(ProtocolError::frame_too_large(big.len(), big.len() - 1))
    );
    Ok(())
}

#[test]
fn test_a_servers_framer_holds_a_clients_header_to_its_rules() -> TestResult {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    // A frame with view 3: a server's frame may carry it, a client's may not.
    let viewed = Credit::new(1).frame(
        Header::new(CLUSTER, Command::Credit)
            .with_stream(3)
            .with_view(3),
    )?;
    let read = feed(
        &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
        &pool,
        &viewed,
        &mut 0,
        1 << 20,
    )
    .context("a server's frame may name a view")?;
    assert_eq!(read.bytes(), &viewed[..]);
    assert!(matches!(
        feed(
            &mut Framer::from_client(CLUSTER, MAX_FRAME_SIZE),
            &pool,
            &viewed,
            &mut 0,
            1 << 20
        )
        .err()
        .context("the refusal")?
        .0,
        TransportErrorEnum::Protocol {
            error: ProtocolError(ProtocolErrorEnum::FieldValue {
                field: "view",
                value: 3,
                ..
            })
        }
    ));
    // Replica 1: a server's frame may name its replica, a client's names 0 (PROTOCOL.md section 2.5, step 6).
    let replica = Credit::new(1).frame(
        Header::new(CLUSTER, Command::Credit)
            .with_stream(3)
            .with_replica(1),
    )?;
    let replica_read = feed(
        &mut Framer::new(CLUSTER, MAX_FRAME_SIZE),
        &pool,
        &replica,
        &mut 0,
        1 << 20,
    )
    .context("a server's frame may name a replica")?;
    assert_eq!(replica_read.bytes(), &replica[..]);
    assert_eq!(
        feed(
            &mut Framer::from_client(CLUSTER, MAX_FRAME_SIZE),
            &pool,
            &replica,
            &mut 0,
            1 << 20
        )
        .err()
        .context("the refusal")?,
        TransportError::protocol(ProtocolError::field_value(14, "replica", 1))
    );
    Ok(())
}

#[test]
fn test_each_frame_finished_expects_the_sequence_after_the_last() -> TestResult {
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let mut framer = Framer::from_client(CLUSTER, MAX_FRAME_SIZE);
    let stream = [credit()?, credit()?, credit()?].concat();
    let mut at = 0;
    let expected = (0..3)
        .map(|_| Ok(feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?.expected_sequence()))
        .collect::<TestResult<Vec<u128>>>()?;
    assert_eq!(expected, vec![1, 2, 3]);
    Ok(())
}
