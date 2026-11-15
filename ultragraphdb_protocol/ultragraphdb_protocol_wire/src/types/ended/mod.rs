/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod ended_body_builder;

/// The sessions whose requests in flight have ended (`PROTOCOL.md` §13.25), ascending by their bytes. The system group
/// takes each mark away and answers nothing: the request's stream ends with it. The command header is all zero.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | sessions, 16 bytes each, at least one |
///
/// [`crate::BodyBuilder::frame`] refuses what [`crate::EndedView`] refuses: no session, a session of all zero, and a
/// session at or below the one before.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ended {
    sessions: Vec<[u8; 16]>,
}

impl Ended {
    /// An Ended naming `sessions`.
    #[must_use]
    pub fn new(sessions: Vec<[u8; 16]>) -> Self {
        Self { sessions }
    }

    /// The sessions, in order.
    #[must_use]
    pub fn sessions(&self) -> &[[u8; 16]] {
        &self.sessions
    }
}
