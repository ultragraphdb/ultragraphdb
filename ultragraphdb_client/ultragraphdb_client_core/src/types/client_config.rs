/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_protocol_wire::{Hello, PROTOCOL_VERSION};

/// What a client is: the cluster it connects to, its name and release, the protocols it speaks, how long it waits
/// for an answer, how often it retries a commit through its session, how far apart, and the credit it grants a
/// result stream at a time.
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
}

impl ClientConfig {
    /// A client of `cluster`: release 0, protocols 1 to this crate's, a 10 s timeout, a commit tried at most 8
    /// times 50 ms apart, credit of 4 frames.
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
        }
    }

    pub fn with_release(mut self, release: u32) -> Self {
        self.release = release;
        self
    }

    pub fn with_protocols(mut self, min: u16, max: u16) -> Self {
        self.protocol_min = min;
        self.protocol_max = max;
        self
    }

    /// How long the client waits for anything it expects before it gives the connection up.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The most times a commit is sent, the first included, at least 1.
    pub fn with_commit_attempts(mut self, attempts: u32) -> Self {
        self.commit_attempts = attempts.max(1);
        self
    }

    /// How long a commit waits before it is sent again after its session answered busy.
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// The Result frames a stream is granted each time its credit runs out, at least 1.
    pub fn with_credit(mut self, frames: u32) -> Self {
        self.credit = frames.max(1);
        self
    }

    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn release(&self) -> u32 {
        self.release
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn commit_attempts(&self) -> u32 {
        self.commit_attempts
    }

    pub fn retry_delay(&self) -> Duration {
        self.retry_delay
    }

    pub fn credit(&self) -> u32 {
        self.credit
    }

    /// The Hello this client opens a connection with.
    pub fn hello(&self) -> Hello {
        Hello::new(
            self.protocol_min,
            self.protocol_max,
            self.release,
            self.name.clone(),
        )
    }
}
