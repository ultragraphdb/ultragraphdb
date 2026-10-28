/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};

use crate::utils::sim::connect;

use crate::utils::ready::ready;

#[test]
fn test_a_read_takes_at_most_what_its_buffer_holds() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (mut client, mut server) = connect(&network);
    let compio::BufResult(written, _) = ready(client.write(b"abcdef".to_vec()));
    assert_eq!(written.unwrap(), 6);
    network.run_until_idle();
    assert_eq!(server.sim().pending(), 6);
    let compio::BufResult(n, buf) = ready(server.read(Vec::with_capacity(4)));
    assert_eq!((n.unwrap(), &buf[..]), (4, &b"abcd"[..]));
    assert_eq!(server.sim().pending(), 2);
    let compio::BufResult(n, buf) = ready(server.read(Vec::with_capacity(4)));
    assert_eq!((n.unwrap(), &buf[..]), (2, &b"ef"[..]));
    assert_eq!(server.sim().pending(), 0);
}

#[test]
fn test_a_read_waits_for_the_peer_to_write() {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (mut client, mut server) = connect(&network);
    let mut read = pin!(server.read(Vec::with_capacity(8)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(matches!(read.as_mut().poll(&mut cx), Poll::Pending));
    let compio::BufResult(written, _) = ready(client.write(b"hi".to_vec()));
    assert_eq!(written.unwrap(), 2);
    match read.as_mut().poll(&mut cx) {
        Poll::Ready(compio::BufResult(n, buf)) => {
            assert_eq!((n.unwrap(), buf), (2, b"hi".to_vec()))
        }
        Poll::Pending => panic!("the read is ready once the peer wrote"),
    }
}
