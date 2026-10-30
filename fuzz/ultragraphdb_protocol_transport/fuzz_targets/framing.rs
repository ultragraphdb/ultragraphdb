/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as what a peer sent on a connection, split into reads as the simulated network splits them, read once as
//! a client reads a server's frames and once as a server reads a client's: the connection reads frames whole, each the
//! next bytes of the stream with a header of this cluster, until it is closed, truncated or refused; nothing panics
//! and every buffer returns to the pool.

#![no_main]

use std::task::{Context, Poll, Waker};

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, NetworkFaults, ReceivedFrame, SimNetwork, SimStream,
    TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{HeaderView, MAX_FRAME_SIZE};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/framing/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The next frame, read as the framer asks from `server`, whose peer has shut down, so a read never waits: the shell
/// of the sans-I/O framer, synchronous.
fn read_frame(
    framer: &mut Framer,
    pool: &BufferPool,
    server: &SimStream,
) -> Result<ReceivedFrame, TransportError> {
    let mut buffer = pool.acquire()?;
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        let mut bytes = buffer.take();
        while bytes.len() < framer.want() {
            match server.poll_chunk(&mut cx, framer.want() - bytes.len()) {
                Poll::Ready(Some(chunk)) => bytes.extend_from_slice(&chunk),
                Poll::Ready(None) | Poll::Pending => break,
            }
        }
        let received = bytes.len();
        let short = received < framer.want();
        buffer.restore(bytes);
        if short {
            return Err(framer.closed(received));
        }
        match framer.advance(&buffer)? {
            FrameStep::Read(_) => {}
            FrameStep::Frame => return Ok(framer.finish(buffer)),
        }
    }
}

/// `stream` sent whole and read through `framer`, the network's splits seeded by `seed`.
fn read_all(seed: u8, stream: &[u8], mut framer: Framer) {
    let network = SimNetwork::new(u64::from(seed), NetworkFaults::none().with_split(500_000));
    let (client, server) = network.connect();
    client.send(stream.to_vec());
    client.shutdown();
    let pool = BufferPool::new(2, MAX_FRAME_SIZE);
    let mut at = 0usize;
    loop {
        match read_frame(&mut framer, &pool, &server) {
            Ok(frame) => {
                let size = HeaderView::new(frame.bytes())
                    .map(|h| h.size() as usize)
                    .unwrap_or(0);
                assert_eq!(frame.bytes().len(), size);
                assert_eq!(
                    frame.bytes(),
                    &stream[at..at + size],
                    "a frame is the stream's next bytes"
                );
                at += size;
                let _ = frame.view();
            }
            Err(error) => {
                match error.0 {
                    TransportErrorEnum::Closed => assert_eq!(at, stream.len()),
                    TransportErrorEnum::Truncated { received, expected } => {
                        assert!(received < expected);
                        assert_eq!(at + received, stream.len());
                    }
                    TransportErrorEnum::Protocol { .. } => {}
                    other => panic!("an unexpected refusal: {other:?}"),
                }
                break;
            }
        }
    }
    assert_eq!(pool.available(), 2);
}

fuzz_target!(|data: &[u8]| {
    // The first byte seeds the network's splits.
    let Some((seed, stream)) = data.split_first() else {
        return;
    };
    read_all(*seed, stream, Framer::new(CLUSTER, MAX_FRAME_SIZE));
    read_all(*seed, stream, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
});
