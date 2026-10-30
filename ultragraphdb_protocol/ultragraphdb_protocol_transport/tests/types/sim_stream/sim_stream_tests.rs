/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};

#[test]
fn test_a_read_takes_at_most_what_its_buffer_holds() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, server) = network.connect();
    let mut cx = Context::from_waker(Waker::noop());
    client.send(b"abcdef".to_vec());
    network.run_until_idle();
    assert_eq!(server.pending(), 6);
    assert_eq!(
        server.poll_chunk(&mut cx, 4),
        Poll::Ready(Some(b"abcd".to_vec()))
    );
    assert_eq!(server.pending(), 2);
    assert_eq!(
        server.poll_chunk(&mut cx, 4),
        Poll::Ready(Some(b"ef".to_vec()))
    );
    assert_eq!(server.pending(), 0);
}

#[test]
fn test_a_read_waits_for_the_peer_to_write() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, server) = network.connect();
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(server.poll_chunk(&mut cx, 8), Poll::Pending);
    client.send(b"hi".to_vec());
    assert_eq!(
        server.poll_chunk(&mut cx, 8),
        Poll::Ready(Some(b"hi".to_vec()))
    );
}

#[test]
fn test_a_read_with_no_room_is_the_end_and_takes_nothing() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, server) = network.connect();
    let mut cx = Context::from_waker(Waker::noop());
    client.send(b"abc".to_vec());
    network.run_until_idle();
    assert_eq!(server.poll_chunk(&mut cx, 0), Poll::Ready(None));
    assert_eq!(server.pending(), 3, "nothing was taken");
    assert_eq!(
        server.poll_chunk(&mut cx, 3),
        Poll::Ready(Some(b"abc".to_vec()))
    );
}

/// A waker counting its wakes.
#[derive(Debug, Default)]
struct Wakes(AtomicUsize);

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn test_a_waiting_read_is_woken_by_a_send_and_by_a_shutdown() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, server) = network.connect();
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut cx = Context::from_waker(&waker);
    assert_eq!(server.poll_chunk(&mut cx, 8), Poll::Pending);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
    client.send(b"hi".to_vec());
    assert_eq!(
        wakes.0.load(Ordering::SeqCst),
        1,
        "the send woke the reader"
    );
    assert_eq!(
        server.poll_chunk(&mut cx, 8),
        Poll::Ready(Some(b"hi".to_vec()))
    );
    assert_eq!(server.poll_chunk(&mut cx, 8), Poll::Pending);
    client.shutdown();
    assert_eq!(wakes.0.load(Ordering::SeqCst), 2, "the shutdown woke it");
    assert_eq!(server.poll_chunk(&mut cx, 8), Poll::Ready(None));
    // A send with no reader waiting wakes nobody.
    server.send(b"x".to_vec());
    assert_eq!(wakes.0.load(Ordering::SeqCst), 2);
}
