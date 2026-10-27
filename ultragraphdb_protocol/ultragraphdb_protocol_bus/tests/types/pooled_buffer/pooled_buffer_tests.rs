/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_bus::{BufferPool, Connection, NetworkFaults, SimNetwork};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Credit, Header};

use crate::utils::ready::ready;

#[test]
fn test_a_buffer_holds_the_frame_read_into_it_and_returns_empty() {
    let pool = BufferPool::new(1, 4_096);
    let fresh = pool.acquire().unwrap();
    assert_eq!(fresh.len(), 0);
    assert!(fresh.as_slice().is_empty());
    assert_eq!(fresh.capacity(), 4_096);
    drop(fresh);
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, server) = network.connect();
    let mut client = Connection::new(client, 7);
    let mut server = Connection::new(server, 7);
    let frame = Credit::new(3)
        .frame(Header::new(7, Command::Credit).with_stream(1))
        .unwrap();
    ready(client.write_frame(frame.clone())).unwrap();
    let received = ready(server.read_frame(&pool)).unwrap();
    assert_eq!(received.bytes(), &frame[..]);
    drop(received);
    let again = pool.acquire().unwrap();
    assert!(again.is_empty(), "a buffer comes back emptied");
}
