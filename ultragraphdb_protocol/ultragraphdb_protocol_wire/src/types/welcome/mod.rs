/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod welcome_body_builder;

use crate::types::error_code::ErrorCode;
use crate::types::error_reply::ErrorReply;
use crate::types::hello_view::HelloView;

/// The server's answer to Hello: the protocol chosen, its release, the largest frame it takes, the streams a connection
/// may hold in flight and the credit each result stream starts with, and its name.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `protocol` | u16, at least 1 |
/// | 4 | `release` | u32 |
/// | 8 | `max_frame` | u32, 4,096 to 1 MiB |
/// | 12 | `max_streams` | u32, at least 1 |
/// | 16 | `initial_credit` | u32 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the server's name, UTF-8 |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome {
    protocol: u16,
    release: u32,
    max_frame: u32,
    max_streams: u32,
    initial_credit: u32,
    server: String,
}

impl Welcome {
    pub fn new(
        protocol: u16,
        release: u32,
        max_frame: u32,
        max_streams: u32,
        initial_credit: u32,
        server: impl Into<String>,
    ) -> Self {
        Self {
            protocol,
            release,
            max_frame,
            max_streams,
            initial_credit,
            server: server.into(),
        }
    }

    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    pub fn release(&self) -> u32 {
        self.release
    }

    pub fn max_frame(&self) -> u32 {
        self.max_frame
    }

    pub fn max_streams(&self) -> u32 {
        self.max_streams
    }

    pub fn initial_credit(&self) -> u32 {
        self.initial_credit
    }

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
