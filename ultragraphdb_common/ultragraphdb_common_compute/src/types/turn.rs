/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Check, Ended, Header, SessionCheck};

/// What one turn sends: the sessions of its Ended frame and the checks of its Check frame, each ascending by session,
/// each check with the ticket its queueing gave.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Turn {
    ended: Vec<[u8; 16]>,
    checks: Vec<(u64, SessionCheck)>,
}

impl Turn {
    /// The turn ending `ended` and checking `checks`, each ascending by session as [`crate::SystemTurns::take`] gives
    /// them.
    pub(crate) fn new(ended: Vec<[u8; 16]>, checks: Vec<(u64, SessionCheck)>) -> Self {
        Self { ended, checks }
    }

    /// The sessions of its Ended frame, ascending.
    #[must_use]
    pub fn ended(&self) -> &[[u8; 16]] {
        &self.ended
    }

    /// The checks of its Check frame with their tickets, ascending by session.
    #[must_use]
    pub fn checks(&self) -> &[(u64, SessionCheck)] {
        &self.checks
    }

    /// Whether it sends nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ended.is_empty() && self.checks.is_empty()
    }

    /// Its Ended frame on `header`'s stream; `None` when it ends no session, or when the frame cannot be built, which a
    /// turn of at most [`crate::SystemTurns::ended_max`] sessions rules out.
    #[must_use]
    pub fn ended_frame(&self, header: Header) -> Option<Vec<u8>> {
        if self.ended.is_empty() {
            return None;
        }
        Ended::new(self.ended.clone()).frame(header).ok()
    }

    /// Its Check frame on `header`'s stream; `None` when it checks no session, or when the frame cannot be built, which
    /// a turn of at most [`crate::SystemTurns::checks_max`] checks rules out and which the link answers each check with
    /// code 17 for.
    #[must_use]
    pub fn check_frame(&self, header: Header) -> Option<Vec<u8>> {
        if self.checks.is_empty() {
            return None;
        }
        let checks = self.checks.iter().map(|(_, check)| *check).collect();
        Check::new(checks).frame(header).ok()
    }
}
