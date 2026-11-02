/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod authenticate_view_body_view;

/// The body of a received Authenticate frame, read in place; see [`crate::Authenticate`] for its layout. Its check
/// holds the message's own rules: a final message is the server's, at an odd step, and only a final message names a
/// principal and an expiry; a mechanism is named at step 0 alone. Which step is due, and from which side, is the
/// receiving connection's to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticateView<'a> {
    step: u32,
    last: bool,
    principal: u64,
    expires: u64,
    mechanism: &'a str,
    message: &'a [u8],
}

impl<'a> AuthenticateView<'a> {
    /// The message's place in its exchange, from 0: the client's even, the server's odd.
    pub fn step(&self) -> u32 {
        self.step
    }

    /// Whether this is the server's last message of the exchange.
    pub fn is_final(&self) -> bool {
        self.last
    }

    /// The principal acted for; 0 without authentication.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// When the credential expires, in microseconds of Unix time; 0 for never.
    pub fn expires(&self) -> u64 {
        self.expires
    }

    /// The SASL mechanism's name, in the client's first message; empty in every other.
    pub fn mechanism(&self) -> &'a str {
        self.mechanism
    }

    /// The mechanism's message, its own bytes.
    pub fn message(&self) -> &'a [u8] {
        self.message
    }
}
