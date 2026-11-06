/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_protocol_wire::{Hello, MAX_FRAME_SIZE, PROTOCOL_VERSION};

/// What a client is: the cluster it connects to, its name and release, the protocols it speaks, how long it waits
/// for an answer, how often it retries a commit through its session, how far apart, the credit it grants a result
/// stream at a time, how long before its credential expires it authenticates again, and the frame buffers it reads into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    cluster: u64,
    name: String,
    release: u32,
    protocol_min: u16,
    protocol_max: u16,
    timeout: Duration,
    commit_attempts: u32,
    retry_delay: Duration,
    credit: u32,
    renewal_margin: Duration,
    buffers: usize,
}

impl ClientConfig {
    /// A client of `cluster`: release 0, protocols 1 to this crate's, a 10 s timeout, a commit tried at most 8
    /// times 50 ms apart, credit of 4 frames, re-authentication 60 s before the credential expires, 64 frame buffers.
    #[must_use]
    pub fn new(cluster: u64, name: impl Into<String>) -> Self {
        Self {
            cluster,
            name: name.into(),
            release: 0,
            protocol_min: 1,
            protocol_max: PROTOCOL_VERSION,
            timeout: Duration::from_secs(10),
            commit_attempts: 8,
            retry_delay: Duration::from_millis(50),
            credit: 4,
            renewal_margin: Duration::from_secs(60),
            buffers: 64,
        }
    }

    /// The client release the Hello names.
    #[must_use]
    pub fn with_release(mut self, release: u32) -> Self {
        self.release = release;
        self
    }

    /// The protocol versions the client speaks, `min` to `max`.
    #[must_use]
    pub fn with_protocols(mut self, min: u16, max: u16) -> Self {
        self.protocol_min = min;
        self.protocol_max = max;
        self
    }

    /// How long the client waits for anything it expects before it gives the connection up. A read waits at least 30 s,
    /// from the later of its last progress and its transaction's deadline, and not while its credit is spent.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The most times a commit is sent, the first included, at least 1; a failed attempt to reconnect while it waits
    /// to be sent again counts as one.
    #[must_use]
    pub fn with_commit_attempts(mut self, attempts: u32) -> Self {
        self.commit_attempts = attempts.max(1);
        self
    }

    /// How long a commit waits before it is sent again after its session answered busy, and how long the shell waits
    /// between attempts to reconnect for a commit waiting to be sent again.
    #[must_use]
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// The Result frames a stream is granted each time its credit runs out, at least 1.
    #[must_use]
    pub fn with_credit(mut self, frames: u32) -> Self {
        self.credit = frames.max(1);
        self
    }

    /// How long before its credential expires the client authenticates again; at most half the credential's
    /// remaining life, so a short-lived credential is renewed in time.
    #[must_use]
    pub fn with_renewal_margin(mut self, margin: Duration) -> Self {
        self.renewal_margin = margin;
        self
    }

    /// The frame buffers the client reads into, at least 2: one takes the frame being read, and the rest hold what the
    /// open reads' credit and refusals can bring, so the client opens a read only while they hold it. A shell sets it to
    /// its pool's.
    #[must_use]
    pub fn with_buffers(mut self, buffers: usize) -> Self {
        self.buffers = buffers.max(2);
        self
    }

    /// The cluster the client connects to.
    #[must_use]
    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    /// The client name the Hello carries.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The client release the Hello names.
    #[must_use]
    pub fn release(&self) -> u32 {
        self.release
    }

    /// How long the client waits for anything it expects.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The most times a commit is sent, the first included.
    #[must_use]
    pub fn commit_attempts(&self) -> u32 {
        self.commit_attempts
    }

    /// How long a commit waits before it is sent again after a busy answer.
    #[must_use]
    pub fn retry_delay(&self) -> Duration {
        self.retry_delay
    }

    /// The Result frames a stream is granted at a time.
    #[must_use]
    pub fn credit(&self) -> u32 {
        self.credit
    }

    /// How long before its credential expires the client authenticates again.
    #[must_use]
    pub fn renewal_margin(&self) -> Duration {
        self.renewal_margin
    }

    /// The frame buffers the client reads into.
    #[must_use]
    pub fn buffers(&self) -> usize {
        self.buffers
    }

    /// The Hello this client opens a connection with.
    #[must_use]
    pub fn hello(&self) -> Hello {
        Hello::new(
            self.protocol_min,
            self.protocol_max,
            self.release,
            MAX_FRAME_SIZE as u32,
            self.name.clone(),
        )
    }
}
