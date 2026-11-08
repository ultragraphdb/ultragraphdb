/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod register_body_builder;

/// A session is registered for the connection's principal (`PROTOCOL.md` §6.1): sent with `session` all zero, answered
/// with the session's identity, 128 random bits, never all zero.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `session` | 16 bytes |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Register {
    session: [u8; 16],
}

impl Register {
    /// A registration of `session`: all zero in a request.
    #[must_use]
    pub fn new(session: [u8; 16]) -> Self {
        Self { session }
    }

    /// The session; all zero in a request.
    #[must_use]
    pub fn session(&self) -> [u8; 16] {
        self.session
    }
}
