/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A [`StorageSession`] that lets its runtime's other tasks run before each push it hands its connection. The tests serve the
//! session on the runtime the test itself runs on, so a session whose pushes are always ready would otherwise starve the test's
//! side of the connection and its timers, and the test would hang where it should fail within its time.

use ultragraphdb_common_role::{Answer, ParkedBytes, Session};
use ultragraphdb_common_storage::StorageSession;
use ultragraphdb_protocol_transport::ReceivedFrame;

/// A storage session yielding once before each push, and otherwise the session it wraps.
pub(crate) struct Yielding(pub(crate) StorageSession);

/// Lets every other task of the runtime run once.
async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            return std::task::Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await;
}

impl Session for Yielding {
    async fn authenticating(&mut self, first: &[u8]) {
        self.0.authenticating(first).await;
    }

    fn authenticated(&mut self, principal: u64) {
        self.0.authenticated(principal);
    }

    fn authentication_refused(&mut self) {
        self.0.authentication_refused();
    }

    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        self.0.answer(stream, frame).await
    }

    fn parked(&self) -> Option<ParkedBytes> {
        self.0.parked()
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        yield_once().await;
        self.0.next_push().await
    }

    fn cancelled(&mut self, stream: u64) {
        self.0.cancelled(stream);
    }

    fn credited(&mut self, stream: u64, frames: u32) {
        self.0.credited(stream, frames);
    }

    fn welcomed(&mut self, max_frame: u32) {
        self.0.welcomed(max_frame);
    }

    fn ready(&mut self, stream: u64) {
        self.0.ready(stream);
    }

    fn left(&mut self, stream: u64) {
        self.0.left(stream);
    }
}
