/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::task::{Context, Poll, Waker};

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
