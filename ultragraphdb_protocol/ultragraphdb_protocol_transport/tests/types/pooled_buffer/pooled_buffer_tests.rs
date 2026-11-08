/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{BufferPool, Framer};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Credit, Header, MAX_FRAME_SIZE};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::feed::feed;

#[test]
fn test_a_buffer_holds_the_frame_read_into_it_and_returns_empty() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let fresh = pool.acquire()?;
    assert_eq!(fresh.len(), 0);
    assert!(fresh.as_slice().is_empty());
    assert_eq!(fresh.capacity(), 4_096);
    drop(fresh);
    let frame = Credit::new(3).frame(Header::new(7, Command::Credit).with_stream(1))?;
    let received = feed(
        &mut Framer::new(7, MAX_FRAME_SIZE),
        &pool,
        &frame,
        &mut 0,
        7,
    )?;
    assert_eq!(received.bytes(), &frame[..]);
    drop(received);
    let again = pool.acquire()?;
    assert!(again.is_empty(), "a buffer comes back emptied");
    Ok(())
}

#[test]
fn test_a_taken_vector_keeps_the_pools_capacity_and_comes_back() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let mut buffer = pool.acquire()?;
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
    Ok(())
}

#[test]
fn test_a_buffer_holding_bytes_counts_them_and_is_not_empty() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let mut buffer = pool.acquire()?;
    let mut bytes = buffer.take();
    // Three bytes put back: its length is the three, and it is not empty.
    bytes.extend_from_slice(b"abc");
    buffer.restore(bytes);
    assert_eq!(buffer.len(), 3);
    assert!(!buffer.is_empty());
    Ok(())
}

/// A buffer whose vector was taken out and never put back, as an owned-buffer read cancelled in flight keeps it, comes
/// back to its pool whole: the next buffer taken holds the pool's capacity, so no later fill reallocates.
#[test]
fn test_a_buffer_lost_with_its_read_comes_back_whole() -> TestResult {
    let capacity = 4_096;
    let pool = BufferPool::new(1, capacity);
    let mut buffer = pool.acquire()?;
    drop(buffer.take());
    drop(buffer);
    let mut again = pool.acquire()?;
    assert!(again.take().capacity() >= capacity, "the pool's capacity");
    Ok(())
}

/// A buffer given back whole is lent again as it was: the pool allocates its buffers once, at start, so the vector a
/// buffer held comes back with the next one taken, and giving a buffer back allocates nothing.
#[test]
fn test_a_buffer_given_back_whole_is_lent_again_without_a_new_allocation() -> TestResult {
    let pool = BufferPool::new(1, 4_096);
    let mut buffer = pool.acquire()?;
    let bytes = buffer.take();
    let lent = bytes.as_ptr();
    buffer.restore(bytes);
    drop(buffer);
    let mut again = pool.acquire()?;
    let relent = again.take();
    assert_eq!(relent.as_ptr(), lent, "the vector the pool lent before");
    Ok(())
}
