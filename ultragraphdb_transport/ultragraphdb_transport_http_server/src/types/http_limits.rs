/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;
use std::time::Duration;

/// How much of a connection a server takes, and how many connections a listener takes at once: past a bound it refuses
/// or waits and keeps nothing more, so a peer holds neither a buffer nor a task for long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpLimits {
    head: usize,
    body: usize,
    time: Duration,
    connections: NonZeroUsize,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<HttpLimits>() <= 64);

impl HttpLimits {
    /// The most a request line and its headers take: a request names a path and a key, never more.
    pub const HEAD_MAX: usize = 4_096;

    /// The most a refused request's connection reads and discards behind its refusal: 32 times the largest body the
    /// config service takes, so a mistaken upload still ends in its refusal, while a peer sending on is cut off.
    pub const DRAIN_MAX: usize = 16 * 1024 * 1024;

    /// The most connections a listener serves at once, 64; the next waits in the listener's backlog until one ends. The
    /// listeners are the control plane's: a role's admin endpoint, which takes an operator's load of a revision, and the
    /// config service, which takes each role's fetch of its settings as it starts and an operator's uploads. A cluster
    /// starting at once asks one fetch per role process, so 64 serves dozens of processes together, each further fetch
    /// waiting its turn rather than failing. Each connection holds at most a head, a body and one read's buffer, so 64 of
    /// the config service's requests of up to 512 KiB hold some tens of MiB at most, and an admin endpoint's of 64 bytes
    /// about 1 MiB. A connection ends within four times its limits' time, its TLS handshake, its request's read, its
    /// response's write and a refusal's drain, the service's call aside, so a waiting one is accepted within that. Go's
    /// `netutil.LimitListener` bounds a listener the same way, by not accepting past its count.
    pub const CONNECTIONS_MAX: NonZeroUsize = NonZeroUsize::MIN.saturating_add(63);

    /// The limits of `body` bytes of body, in `time` to read the request, again to write the response, and again to discard
    /// what a peer still sends behind a refusal, and of [`Self::CONNECTIONS_MAX`] connections at once.
    #[must_use]
    pub fn new(body: usize, time: Duration) -> Self {
        Self {
            head: Self::HEAD_MAX,
            body,
            time,
            connections: Self::CONNECTIONS_MAX,
        }
    }

    /// These limits, of at most `connections` connections at once.
    #[must_use]
    pub fn with_connections(self, connections: NonZeroUsize) -> Self {
        Self {
            connections,
            ..self
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

    /// How long reading a request may take, and writing a response, and discarding what follows a refusal.
    #[must_use]
    pub fn time(&self) -> Duration {
        self.time
    }

    /// The most connections a listener serves at once.
    #[must_use]
    pub fn connections(&self) -> NonZeroUsize {
        self.connections
    }
}
