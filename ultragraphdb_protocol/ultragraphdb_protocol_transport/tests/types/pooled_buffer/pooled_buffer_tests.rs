/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{BufferPool, Framer};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Credit, Header, MAX_FRAME_SIZE};

use crate::utils::feed::feed;

#[test]
fn test_a_buffer_holds_the_frame_read_into_it_and_returns_empty() {
    let pool = BufferPool::new(1, 4_096);
    let fresh = pool.acquire().unwrap();
    assert_eq!(fresh.len(), 0);
    assert!(fresh.as_slice().is_empty());
    assert_eq!(fresh.capacity(), 4_096);
    drop(fresh);
    let frame = Credit::new(3)
        .frame(Header::new(7, Command::Credit).with_stream(1))
        .unwrap();
    let received = feed(
        &mut Framer::new(7, MAX_FRAME_SIZE),
        &pool,
        &frame,
        &mut 0,
        7,
    )
    .unwrap();
    assert_eq!(received.bytes(), &frame[..]);
    drop(received);
    let again = pool.acquire().unwrap();
    assert!(again.is_empty(), "a buffer comes back emptied");
}

#[test]
fn test_a_taken_vector_keeps_the_pools_capacity_and_comes_back() {
    let pool = BufferPool::new(1, 4_096);
    let mut buffer = pool.acquire().unwrap();
    let mut bytes = buffer.take();
    assert!(
        bytes.capacity() >= 4_096,
        "reading into it never reallocates"
    );
    bytes.extend_from_slice(b"abc");
    buffer.restore(bytes);
    assert_eq!(buffer.as_slice(), b"abc");
    drop(buffer);
    assert_eq!(pool.available(), 1);
}
