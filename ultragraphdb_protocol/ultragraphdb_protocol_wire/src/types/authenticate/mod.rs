/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod authenticate_body_builder;

/// One message of a SASL exchange (RFC 4422) on stream 0: the client's first names the mechanism, and each side
/// answers the other's until the server's final message, which names the principal the connection acts for and when
/// its credential expires, or its refusal.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `step` | u32, the message's place in its exchange from 0: the client's even, the server's odd |
/// | 4 | `final` | u8 boolean: the server's last message of the exchange |
/// | 8 | `principal` | u64: in a final message, the principal; 0 otherwise |
/// | 16 | `expires` | u64: in a final message, the credential's expiry in microseconds of Unix time, 0 for never; 0 otherwise |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the mechanism's name, UTF-8, in the client's message of step 0; empty in every other |
/// | 1 | the mechanism's message, its own bytes |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticate {
    step: u32,
    last: bool,
    principal: u64,
    expires: u64,
    mechanism: String,
    message: Vec<u8>,
}

impl Authenticate {
    /// The message of `step` carrying the mechanism's `message`; the client's of step 0 names its `mechanism`, every
    /// other an empty one.
    pub fn new(step: u32, mechanism: impl Into<String>, message: Vec<u8>) -> Self {
        Self {
            step,
            last: false,
            principal: 0,
            expires: 0,
            mechanism: mechanism.into(),
            message,
        }
    }

    /// The server's final message, naming the `principal` the connection acts for and when its credential `expires`,
    /// in microseconds of Unix time, 0 for never.
    pub fn with_final(mut self, principal: u64, expires: u64) -> Self {
        self.last = true;
        self.principal = principal;
        self.expires = expires;
        self
    }

    pub fn step(&self) -> u32 {
        self.step
    }

    /// Whether this is the server's final message of the exchange.
    pub fn is_final(&self) -> bool {
        self.last
    }

    pub fn principal(&self) -> u64 {
        self.principal
    }

    pub fn expires(&self) -> u64 {
        self.expires
    }

    pub fn mechanism(&self) -> &str {
        &self.mechanism
    }

    pub fn message(&self) -> &[u8] {
        &self.message
    }
}
