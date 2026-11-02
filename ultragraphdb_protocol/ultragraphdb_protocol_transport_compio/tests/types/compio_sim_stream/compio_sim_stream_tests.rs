/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::sim::connect;

use crate::utils::ready::ready;

#[test]
fn test_a_read_takes_at_most_what_its_buffer_holds() -> TestResult {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (mut client, mut server) = connect(&network);
    let compio::BufResult(written, _) = ready(client.write(b"abcdef".to_vec()));
    assert_eq!(written?, 6);
    network.run_until_idle();
    assert_eq!(server.sim().pending(), 6);
    let compio::BufResult(first_read, first_buf) = ready(server.read(Vec::with_capacity(4)));
    assert_eq!((first_read?, &first_buf[..]), (4, &b"abcd"[..]));
    assert_eq!(server.sim().pending(), 2);
    let compio::BufResult(second_read, second_buf) = ready(server.read(Vec::with_capacity(4)));
    assert_eq!((second_read?, &second_buf[..]), (2, &b"ef"[..]));
    assert_eq!(server.sim().pending(), 0);
    Ok(())
}

#[test]
fn test_a_read_waits_for_the_peer_to_write() -> TestResult {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (mut client, mut server) = connect(&network);
    let mut read = pin!(server.read(Vec::with_capacity(8)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(matches!(read.as_mut().poll(&mut cx), Poll::Pending));
    let compio::BufResult(written, _) = ready(client.write(b"hi".to_vec()));
    assert_eq!(written?, 2);
    let polled = read.as_mut().poll(&mut cx);
    assert!(
        matches!(polled, Poll::Ready(_)),
        "the read is ready once the peer wrote"
    );
    if let Poll::Ready(compio::BufResult(n, buf)) = polled {
        assert_eq!((n?, buf), (2, b"hi".to_vec()));
    }
    Ok(())
}
