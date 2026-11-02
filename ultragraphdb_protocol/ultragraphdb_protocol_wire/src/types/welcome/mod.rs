/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod welcome_body_builder;

use crate::types::error_code::ErrorCode;
use crate::types::error_reply::ErrorReply;
use crate::types::hello_view::HelloView;
use crate::utils::constants::{
    DEFAULT_INITIAL_CREDIT, DEFAULT_MAX_STREAMS, DEFAULT_MAX_WRITE_SET, DEFAULT_TRANSACTION_MS,
    MAX_FRAME_SIZE,
};

/// The server's answer to Hello: the protocol chosen, its release, whether the connection authenticates and by which
/// SASL mechanisms, the largest frame it takes, the streams a connection may hold in flight, the credit each read's
/// stream starts with, the largest write set a transaction commits, a read-write transaction's longest life, and its
/// name.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `protocol` | u16, at least 1 |
/// | 2 | `auth` | u8: 1 the connection authenticates, 0 not |
/// | 4 | `release` | u32 |
/// | 8 | `max_frame` | u32, [`crate::MAX_FRAME_FLOOR`] to [`crate::MAX_FRAME_SIZE`] |
/// | 12 | `max_streams` | u32, at least 1 |
/// | 16 | `initial_credit` | u32 |
/// | 20 | `max_write_set` | u32, bytes |
/// | 24 | `transaction_ms` | u32, milliseconds |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the server's name, UTF-8 |
/// | 1 | the mechanisms: string list `ends` |
/// | 2 | the mechanisms: string list `text`, each a SASL mechanism name |
///
/// `auth` follows from the mechanisms: a server that lists one authenticates its connections, and one listing none
/// runs without authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome {
    protocol: u16,
    release: u32,
    max_frame: u32,
    max_streams: u32,
    initial_credit: u32,
    max_write_set: u32,
    transaction_ms: u32,
    mechanisms: Vec<String>,
    server: String,
}

impl Welcome {
    /// The Welcome of a server named `server` speaking up to `protocol` at `release`, with the protocol's default
    /// limits (`PROTOCOL.md` §11.9) and no authentication.
    pub fn new(protocol: u16, release: u32, server: impl Into<String>) -> Self {
        Self {
            protocol,
            release,
            max_frame: MAX_FRAME_SIZE as u32,
            max_streams: DEFAULT_MAX_STREAMS,
            initial_credit: DEFAULT_INITIAL_CREDIT,
            max_write_set: DEFAULT_MAX_WRITE_SET,
            transaction_ms: DEFAULT_TRANSACTION_MS,
            mechanisms: Vec::new(),
            server: server.into(),
        }
    }

    /// The answer with the largest frame the server takes.
    pub fn with_max_frame(mut self, max_frame: u32) -> Self {
        self.max_frame = max_frame;
        self
    }

    /// The answer with the streams a connection may hold in flight.
    pub fn with_max_streams(mut self, max_streams: u32) -> Self {
        self.max_streams = max_streams;
        self
    }

    /// The answer with the credit each read's stream starts with.
    pub fn with_initial_credit(mut self, initial_credit: u32) -> Self {
        self.initial_credit = initial_credit;
        self
    }

    /// The answer with the largest write set a transaction commits.
    pub fn with_max_write_set(mut self, max_write_set: u32) -> Self {
        self.max_write_set = max_write_set;
        self
    }

    /// The answer with a read-write transaction's longest life, in milliseconds.
    pub fn with_transaction_ms(mut self, transaction_ms: u32) -> Self {
        self.transaction_ms = transaction_ms;
        self
    }

    /// The Welcome of a server authenticating its connections by `mechanisms`, SASL mechanism names in the server's
    /// order of preference; none runs without authentication.
    pub fn with_mechanisms<S: Into<String>>(
        mut self,
        mechanisms: impl IntoIterator<Item = S>,
    ) -> Self {
        self.mechanisms = mechanisms.into_iter().map(Into::into).collect();
        self
    }

    /// The protocol version chosen.
    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    /// The server's release.
    pub fn release(&self) -> u32 {
        self.release
    }

    /// Whether the connection authenticates before its first request: the server lists a mechanism.
    pub fn authenticates(&self) -> bool {
        !self.mechanisms.is_empty()
    }

    /// The SASL mechanisms the server offers; none when it runs without authentication.
    pub fn mechanisms(&self) -> &[String] {
        &self.mechanisms
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
    pub fn server(&self) -> &str {
        &self.server
    }

    /// The answer of a server whose highest protocol is this Welcome's to `hello`: this Welcome at the highest protocol
    /// both speak, or, when the client's range lies above it, an Error of [`crate::ErrorCode::ProtocolVersion`]
    /// naming the server's highest protocol. Every server speaks every protocol from 1 to its highest.
    pub fn answer(&self, hello: &HelloView<'_>) -> Result<Welcome, ErrorReply> {
        if hello.protocol_min() > self.protocol {
            let message = format!(
                "the client speaks protocols {} to {}; this server speaks 1 to {}",
                hello.protocol_min(),
                hello.protocol_max(),
                self.protocol
            );
            return Err(ErrorReply::new(ErrorCode::ProtocolVersion, message)
                .with_detail(self.protocol.into()));
        }
        let mut welcome = self.clone();
        welcome.protocol = self.protocol.min(hello.protocol_max());
        Ok(welcome)
    }
}
