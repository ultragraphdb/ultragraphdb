/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The tests' executor for the simulated network: a future polled until it is ready, at most `POLLS` times. A
//! simulated stream needs no wake-up from outside the test, since a read advances the network itself, so a future
//! still pending after that many polls would wait forever: the test fails at once instead of hanging.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

/// More polls than any conversation of these tests takes: each poll of a waiting read either completes it or finds
/// nothing in flight.
const POLLS: usize = 10_000;

/// `future`'s output.
///
/// # Panics
///
/// When `future` is still pending after `POLLS` polls: it waits for bytes that never arrive.
pub fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..POLLS {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
    }
    panic!("the future is still pending after {POLLS} polls: it waits for bytes that never arrive");
}
