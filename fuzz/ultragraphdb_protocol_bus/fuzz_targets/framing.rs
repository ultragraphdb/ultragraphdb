/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as what a peer sent on a connection, split into reads as the simulated network splits them: the
//! connection reads frames whole, each the next bytes of the stream with a header of this cluster, until it is closed,
//! truncated or refused; nothing panics and every buffer returns to the pool.

#![no_main]

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use compio::io::AsyncWrite;
use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_bus::{BufferPool, BusErrorEnum, Connection, NetworkFaults, SimNetwork};
use ultragraphdb_protocol_wire::{HeaderView, MAX_FRAME_SIZE};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/framing/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// `future` polled to its end: with the peer shut down, a simulated read never waits.
fn now<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // The first byte seeds the network's splits.
    let Some((seed, stream)) = data.split_first() else {
        return;
    };
    let network = SimNetwork::new(u64::from(*seed), NetworkFaults::none().with_split(500_000));
    let (mut client, server) = network.connect();
    let compio::BufResult(written, _) = now(client.write(stream.to_vec()));
    assert_eq!(written.ok(), Some(stream.len()));
    now(client.shutdown()).expect("a simulated shutdown");
    let pool = BufferPool::new(2, MAX_FRAME_SIZE);
    let mut connection = Connection::new(server, CLUSTER);
    let mut at = 0usize;
    loop {
        match now(connection.read_frame(&pool)) {
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
                    BusErrorEnum::Closed => assert_eq!(at, stream.len()),
                    BusErrorEnum::Truncated { received, expected } => {
                        assert!(received < expected);
                        assert_eq!(at + received, stream.len());
                    }
                    BusErrorEnum::Protocol { .. } => {}
                    other => panic!("an unexpected refusal: {other:?}"),
                }
                break;
            }
        }
    }
    assert_eq!(pool.available(), 2);
});
