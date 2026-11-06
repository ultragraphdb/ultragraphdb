/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_protocol_wire::{DEFAULT_INITIAL_CREDIT, DEFAULT_TRANSACTION_MS};

/// What bounds a client connection's requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointLimits {
    held_bytes: u64,
    scan_stall: Duration,
    transaction_ms: u32,
    initial_credit: u32,
    statements: usize,
    refusal_interval: Duration,
}

impl EndpointLimits {
    /// How long a principal past its failed authentications is refused when the settings name no other: 15 minutes
    /// (`PROTOCOL.md` §11.9).
    pub const REFUSAL_INTERVAL_DEFAULT: Duration = Duration::from_secs(900);

    /// The limits of a connection holding back `held_bytes` of writes behind its reads, taking a compute process for hung after
    /// `scan_stall`, each the protocol's default else (`PROTOCOL.md` §11.9).
    #[must_use]
    pub fn new(held_bytes: u64, scan_stall: Duration) -> Self {
        Self {
            held_bytes,
            scan_stall,
            transaction_ms: DEFAULT_TRANSACTION_MS,
            initial_credit: DEFAULT_INITIAL_CREDIT,
            statements: ultragraphdb_protocol_wire::StatementCache::DEFAULT_CAPACITY,
            refusal_interval: Self::REFUSAL_INTERVAL_DEFAULT,
        }
    }

    /// The limits with a read-write transaction living `transaction_ms`, as the Welcome states.
    #[must_use]
    pub fn with_transaction_ms(mut self, transaction_ms: u32) -> Self {
        self.transaction_ms = transaction_ms;
        self
    }

    /// The limits with a read's stream starting with `initial_credit`, as the Welcome states.
    #[must_use]
    pub fn with_initial_credit(mut self, initial_credit: u32) -> Self {
        self.initial_credit = initial_credit;
        self
    }

    /// The limits with a principal past its failed authentications refused for `refusal_interval`.
    #[must_use]
    pub fn with_refusal_interval(mut self, refusal_interval: Duration) -> Self {
        self.refusal_interval = refusal_interval;
        self
    }

    /// How long a principal past its failed authentications is refused.
    #[must_use]
    pub fn refusal_interval(&self) -> Duration {
        self.refusal_interval
    }

    /// The bytes of Write and Operation frames a connection's reads hold back.
    #[must_use]
    pub fn held_bytes(&self) -> u64 {
        self.held_bytes
    }

    /// How long a compute process sends no frame of a read, past its deadline, before it counts as hung.
    #[must_use]
    pub fn scan_stall(&self) -> Duration {
        self.scan_stall
    }

    /// The longest a read-write transaction lives, in milliseconds.
    #[must_use]
    pub fn transaction_ms(&self) -> u32 {
        self.transaction_ms
    }

    /// What a read's stream starts with.
    #[must_use]
    pub fn initial_credit(&self) -> u32 {
        self.initial_credit
    }

    /// The statements a connection's cache holds.
    #[must_use]
    pub fn statements(&self) -> usize {
        self.statements
    }
}
