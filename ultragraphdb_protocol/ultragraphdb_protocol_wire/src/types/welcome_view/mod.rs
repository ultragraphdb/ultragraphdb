/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod welcome_view_body_view;

use crate::types::string_list::StringList;

/// The body of a received Welcome frame, read in place; see [`crate::Welcome`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelcomeView<'a> {
    protocol: u16,
    authenticates: bool,
    release: u32,
    max_frame: u32,
    max_streams: u32,
    initial_credit: u32,
    max_write_set: u32,
    transaction_ms: u32,
    server: &'a str,
    mechanisms: StringList<'a>,
}

impl<'a> WelcomeView<'a> {
    /// The protocol version chosen.
    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    /// Whether the connection authenticates before its first request, the header's `auth`.
    pub fn authenticates(&self) -> bool {
        self.authenticates
    }

    /// The server's release.
    pub fn release(&self) -> u32 {
        self.release
    }

    /// The largest frame the sender takes, in bytes.
    pub fn max_frame(&self) -> u32 {
        self.max_frame
    }

    /// The streams a connection may hold in flight.
    pub fn max_streams(&self) -> u32 {
        self.max_streams
    }

    /// The credit each read's stream starts with.
    pub fn initial_credit(&self) -> u32 {
        self.initial_credit
    }

    /// The largest write set a transaction commits, in bytes.
    pub fn max_write_set(&self) -> u32 {
        self.max_write_set
    }

    /// A read-write transaction's longest life, in milliseconds.
    pub fn transaction_ms(&self) -> u32 {
        self.transaction_ms
    }

    /// The server's name.
    pub fn server(&self) -> &'a str {
        self.server
    }

    /// The SASL mechanisms the server accepts, in its order; empty without authentication.
    pub fn mechanisms(&self) -> &StringList<'a> {
        &self.mechanisms
    }
}
