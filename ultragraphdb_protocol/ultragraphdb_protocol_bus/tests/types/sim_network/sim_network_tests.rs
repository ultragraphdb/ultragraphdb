/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_protocol_bus::{NetworkFaults, SimNetwork, SimStream};

use crate::utils::ready::ready;

fn send(stream: &mut SimStream, message: &[u8]) {
    let compio::BufResult(written, _) = ready(stream.write(message.to_vec()));
    assert_eq!(written.unwrap(), message.len());
}

/// Every chunk that has arrived at `stream`, one per read.
fn drain(stream: &mut SimStream) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    while stream.pending() > 0 {
        let compio::BufResult(n, buf) = ready(stream.read(Vec::with_capacity(1 << 16)));
        let n = n.unwrap();
        assert_eq!(n, buf.len());
        chunks.push(buf);
    }
    chunks
}

fn run(
    seed: u64,
    faults: NetworkFaults,
    messages: usize,
) -> (Vec<Vec<u8>>, u64, ultragraphdb_protocol_bus::NetworkStats) {
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
    let (chunks, _, stats) = run(1, NetworkFaults::none().with_drop(1_000_000), 10);
    assert!(chunks.is_empty());
    assert_eq!((stats.sent(), stats.dropped()), (10, 10));
    // Every message twice.
    let (chunks, _, stats) = run(1, NetworkFaults::none().with_duplicate(1_000_000), 3);
    assert_eq!(
        chunks,
        vec![
            vec![0; 10],
            vec![0; 10],
            vec![1; 11],
            vec![1; 11],
            vec![2; 12],
            vec![2; 12]
        ]
    );
    assert_eq!(stats.duplicated(), 3);
    // Every message split into several chunks that join back in order.
    let (chunks, _, stats) = run(1, NetworkFaults::none().with_split(1_000_000), 5);
    assert!(chunks.len() > 5);
    assert_eq!(stats.split(), 5);
    let joined: Vec<u8> = chunks.concat();
    let sent: Vec<u8> = (0..5).flat_map(|i| vec![i as u8; 10 + i]).collect();
    assert_eq!(joined, sent);
    // Every message delayed, the order kept and the clock advanced past one step.
    let (chunks, now, stats) = run(1, NetworkFaults::none().with_delay(1_000_000, 8), 6);
    assert_eq!(
        chunks,
        (0..6).map(|i| vec![i as u8; 10 + i]).collect::<Vec<_>>()
    );
    assert_eq!(stats.delayed(), 6);
    assert!(now > 1 && now <= 9, "{now}");
}

#[test]
fn test_a_read_with_nothing_arrived_advances_the_clock_until_its_message_arrives() {
    let network = SimNetwork::new(5, NetworkFaults::none().with_delay(1_000_000, 4));
    let (mut client, mut server) = network.connect();
    send(&mut client, b"late");
    let compio::BufResult(n, buf) = ready(server.read(Vec::with_capacity(16)));
    assert_eq!(n.unwrap(), 4);
    assert_eq!(buf, b"late");
    assert!(network.now() >= 1);
}

#[test]
fn test_a_shut_down_side_reads_as_the_end_once_drained() {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (mut client, mut server) = network.connect();
    send(&mut client, b"last");
    ready(client.shutdown()).unwrap();
    ready(client.flush()).unwrap();
    let compio::BufResult(n, buf) = ready(server.read(Vec::with_capacity(16)));
    assert_eq!((n.unwrap(), buf), (4, b"last".to_vec()));
    let compio::BufResult(n, _) = ready(server.read(Vec::with_capacity(16)));
    assert_eq!(n.unwrap(), 0);
}
