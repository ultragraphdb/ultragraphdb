/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::task::{Context, Poll, Waker};

use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork, SimStream};

fn send(stream: &mut SimStream, message: &[u8]) {
    stream.send(message.to_vec());
}

/// A read of at most `room` bytes, ready or not.
fn read(stream: &SimStream, room: usize) -> Poll<Option<Vec<u8>>> {
    stream.poll_chunk(&mut Context::from_waker(Waker::noop()), room)
}

/// Every chunk that has arrived at `stream`, one per read.
fn drain(stream: &mut SimStream) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    while stream.pending() > 0 {
        let chunk = read(stream, 1 << 16);
        assert!(
            matches!(chunk, Poll::Ready(Some(_))),
            "arrived bytes are ready: {chunk:?}"
        );
        let Poll::Ready(Some(buf)) = chunk else {
            break;
        };
        chunks.push(buf);
    }
    chunks
}

fn run(
    seed: u64,
    faults: NetworkFaults,
    messages: usize,
) -> (
    Vec<Vec<u8>>,
    u64,
    ultragraphdb_protocol_transport::NetworkStats,
) {
    let network = SimNetwork::new(seed, faults);
    let (mut client, mut server) = network.connect();
    for i in 0..messages {
        send(&mut client, &vec![i as u8; 10 + i]);
    }
    network.run_until_idle();
    (drain(&mut server), network.now(), network.stats())
}

#[test]
fn test_without_faults_messages_arrive_whole_and_in_order_after_one_step() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (mut client, mut server) = network.connect();
    send(&mut client, b"one");
    send(&mut client, b"two");
    assert_eq!(
        server.pending(),
        0,
        "nothing arrives before the clock moves"
    );
    network.step();
    assert_eq!(network.now(), 1);
    assert_eq!(server.pending(), 6);
    assert_eq!(drain(&mut server), vec![b"one".to_vec(), b"two".to_vec()]);
    // The other direction is its own link.
    send(&mut server, b"back");
    network.run_until_idle();
    assert_eq!(drain(&mut client), vec![b"back".to_vec()]);
    assert_eq!(network.stats().sent(), 3);
}

#[test]
fn test_a_seed_replays_its_faults() {
    let faults = NetworkFaults::none()
        .with_drop(200_000)
        .with_duplicate(200_000)
        .with_delay(300_000, 5)
        .with_split(500_000);
    let a = run(42, faults, 100);
    let b = run(42, faults, 100);
    assert_eq!(a, b);
    let c = run(43, faults, 100);
    assert_ne!(a.0, c.0, "another seed draws other faults");
}

#[test]
fn test_each_fault_does_what_it_says() {
    // Every message dropped.
    let (dropped, _, dropped_stats) = run(1, NetworkFaults::none().with_drop(1_000_000), 10);
    assert!(dropped.is_empty());
    assert_eq!((dropped_stats.sent(), dropped_stats.dropped()), (10, 10));
    // Every message twice.
    let (duplicated, _, duplicated_stats) =
        run(1, NetworkFaults::none().with_duplicate(1_000_000), 3);
    assert_eq!(
        duplicated,
        vec![
            vec![0; 10],
            vec![0; 10],
            vec![1; 11],
            vec![1; 11],
            vec![2; 12],
            vec![2; 12]
        ]
    );
    assert_eq!(duplicated_stats.duplicated(), 3);
    // Every message split into several chunks that join back in order.
    let (split, _, split_stats) = run(1, NetworkFaults::none().with_split(1_000_000), 5);
    assert!(split.len() > 5);
    assert_eq!(split_stats.split(), 5);
    let joined: Vec<u8> = split.concat();
    let sent: Vec<u8> = (0..5).flat_map(|i| vec![i as u8; 10 + i]).collect();
    assert_eq!(joined, sent);
    // Every message delayed, the order kept and the clock advanced past one step.
    let (delayed, now, delayed_stats) = run(1, NetworkFaults::none().with_delay(1_000_000, 8), 6);
    assert_eq!(
        delayed,
        (0..6).map(|i| vec![i as u8; 10 + i]).collect::<Vec<_>>()
    );
    assert_eq!(delayed_stats.delayed(), 6);
    assert!(now > 1 && now <= 9, "{now}");
}

#[test]
fn test_a_read_with_nothing_arrived_advances_the_clock_until_its_message_arrives() {
    let network = SimNetwork::new(5, NetworkFaults::none().with_delay(1_000_000, 4));
    let (mut client, server) = network.connect();
    send(&mut client, b"late");
    assert_eq!(read(&server, 16), Poll::Ready(Some(b"late".to_vec())));
    assert!(network.now() >= 1);
}

#[test]
fn test_a_shut_down_side_reads_as_the_end_once_drained() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (mut client, server) = network.connect();
    send(&mut client, b"last");
    client.shutdown();
    assert_eq!(read(&server, 16), Poll::Ready(Some(b"last".to_vec())));
    assert_eq!(read(&server, 16), Poll::Ready(None));
}

/// Every chunk `stream` reads until the end, its peer having shut its side down: empty chunks included.
fn read_to_end(stream: &SimStream) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    while let Poll::Ready(Some(chunk)) = read(stream, 1 << 16) {
        chunks.push(chunk);
    }
    chunks
}

#[test]
fn test_a_delayed_message_arrives_within_one_to_max_delay_steps_and_takes_each() {
    // Every message delayed, at most 2 steps: the message arrives after the first step or the second, never later, and
    // over 64 seeds both happen.
    let mut steps_taken = Vec::new();
    for seed in 0..64 {
        let network = SimNetwork::new(seed, NetworkFaults::none().with_delay(1_000_000, 2));
        let (mut client, server) = network.connect();
        send(&mut client, b"m");
        let mut steps = 0;
        while server.pending() == 0 {
            network.step();
            steps += 1;
            assert!(steps <= 2, "seed {seed}: no arrival after 2 steps");
        }
        steps_taken.push(steps);
    }
    assert!(steps_taken.contains(&1), "{steps_taken:?}");
    assert!(steps_taken.contains(&2), "{steps_taken:?}");
}

#[test]
fn test_a_split_message_arrives_in_two_to_its_length_non_empty_chunks() {
    // Every message split: a 3-byte message has two places to cut, so it arrives as 2 or 3 chunks, none empty, that
    // join back to it; over 64 seeds both counts happen.
    let mut counts = Vec::new();
    for seed in 0..64 {
        let network = SimNetwork::new(seed, NetworkFaults::none().with_split(1_000_000));
        let (mut client, server) = network.connect();
        send(&mut client, b"abc");
        client.shutdown();
        let chunks = read_to_end(&server);
        assert!(
            chunks.iter().all(|chunk| !chunk.is_empty()),
            "seed {seed}: {chunks:?}"
        );
        assert_eq!(chunks.concat(), b"abc", "seed {seed}");
        counts.push(chunks.len());
    }
    assert!(counts.iter().all(|n| (2..=3).contains(n)), "{counts:?}");
    assert!(counts.contains(&2) && counts.contains(&3), "{counts:?}");
}

#[test]
fn test_a_one_byte_message_is_never_split() {
    // One byte has nowhere to cut: it arrives whole, and no split is counted.
    let network = SimNetwork::new(1, NetworkFaults::none().with_split(1_000_000));
    let (mut client, server) = network.connect();
    send(&mut client, b"x");
    client.shutdown();
    assert_eq!(read_to_end(&server), vec![b"x".to_vec()]);
    assert_eq!(network.stats().split(), 0);
}

#[test]
fn test_a_seed_draws_splitmix64s_stream() {
    // Seed 0, every message dropped at 500,000 per million: each send draws once for the drop and, kept, once each for
    // the duplicate and the delay (rates 0 draw too); a one-byte message draws nothing to split. The messages kept are
    // those of SplitMix64 (Vigna's splitmix64.c) computed in Python from seed 0, whose first output is the reference's
    // 0xE220A8397B1DCDAF: a draw kept when `draw % 1_000_000 >= 500_000`.
    let network = SimNetwork::new(0, NetworkFaults::none().with_drop(500_000));
    let (mut client, server) = network.connect();
    for i in 0..32u8 {
        send(&mut client, &[i]);
    }
    client.shutdown();
    let arrived: Vec<u8> = read_to_end(&server).concat();
    assert_eq!(
        arrived,
        vec![
            0, 1, 4, 6, 9, 10, 11, 12, 16, 18, 20, 21, 22, 23, 24, 25, 26
        ]
    );
    assert_eq!(network.stats().dropped(), 15);
}

#[test]
fn test_a_fault_happens_for_a_draw_below_its_rate_and_not_at_it() {
    // Seed 0's first draw is 0xE220A8397B1DCDAF, 607,535 modulo a million (Python, as above): a drop rate of 607,535
    // keeps the message, one of 607,536 drops it.
    for (rate, dropped) in [(607_535, 0), (607_536, 1)] {
        let network = SimNetwork::new(0, NetworkFaults::none().with_drop(rate));
        let (mut client, _server) = network.connect();
        send(&mut client, b"m");
        assert_eq!(network.stats().dropped(), dropped, "rate {rate}");
    }
}
