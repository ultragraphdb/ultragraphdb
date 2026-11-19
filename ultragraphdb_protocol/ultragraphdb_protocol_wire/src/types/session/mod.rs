/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod session_body_builder;

/// Compute registers a session for a principal in the metadata state machine's registry (`PROTOCOL.md` §13.23):
/// the identity it drew and the principal the session belongs to. The system group answers with the same bytes once
/// the registration is durable.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `session` | 16 bytes: the session's identity, not all zero |
/// | 16 | `principal` | u64: the principal the session belongs to |
///
/// No sections. [`crate::BodyBuilder::frame`] refuses a `session` of all zero as its view does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Session {
    session: [u8; 16],
    principal: u64,
}

impl Session {
    /// The registration of `session` for `principal`, and its reply.
    #[must_use]
    pub fn new(session: [u8; 16], principal: u64) -> Self {
        Self { session, principal }
    }

    /// The session's identity.
    #[must_use]
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The principal the session belongs to.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }
}
