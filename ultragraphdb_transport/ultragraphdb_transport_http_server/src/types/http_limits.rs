/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

/// How much of a connection a server takes: past a bound it refuses without reading further, so a peer holds neither a
/// buffer nor a task for long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpLimits {
    head: usize,
    body: usize,
    time: Duration,
}

impl HttpLimits {
    /// The most a request line and its headers take: a request names a path and a key, never more.
    pub const HEAD_MAX: usize = 4_096;

    /// The limits of `body` bytes of body, in `time` to read the request and again to write the response.
    #[must_use]
    pub fn new(body: usize, time: Duration) -> Self {
        Self {
            head: Self::HEAD_MAX,
            body,
            time,
        }
    }

    /// The most a head takes.
    #[must_use]
    pub fn head(&self) -> usize {
        self.head
    }

    /// The most a body takes.
    #[must_use]
    pub fn body(&self) -> usize {
        self.body
    }

    /// How long reading a request may take, and writing a response.
    #[must_use]
    pub fn time(&self) -> Duration {
        self.time
    }
}
