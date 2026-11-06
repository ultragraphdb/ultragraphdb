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

/// A simulated stream splits as a socket does: a read pending on one half holds up no write on the other, and each half
/// speaks for the same end.
#[test]
fn test_its_halves_read_and_write_the_same_end_at_once() -> TestResult {
    use compio::io::util::Splittable;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Waker};
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, mut server) = connect(&network);
    let (mut read_half, mut write_half) = client.split();
    let mut read = pin!(read_half.read(Vec::with_capacity(8)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(read.as_mut().poll(&mut cx).is_pending());
    let compio::BufResult(written, _) = ready(write_half.write(b"up".to_vec()));
    assert_eq!(written?, 2);
    let compio::BufResult(got, up) = ready(server.read(Vec::with_capacity(8)));
    assert_eq!((got?, up), (2, b"up".to_vec()));
    let compio::BufResult(answered, _) = ready(server.write(b"down".to_vec()));
    assert_eq!(answered?, 4);
    let compio::BufResult(n, down) = ready(read);
    assert_eq!((n?, down), (4, b"down".to_vec()));
    Ok(())
}
