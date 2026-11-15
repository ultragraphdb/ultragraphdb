/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod check_body_builder;

use crate::types::session_check::SessionCheck;

/// The owner checks of one turn of an endpoint core (`PROTOCOL.md` §13.24): one check per session, ascending by the
/// sessions' bytes. The system group decides them in their order and answers the frame whole, every field as sent and
/// each `outcome` filled. The command header is all zero.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | checks, 32 bytes each ([`SessionCheck`]), at least one |
///
/// [`crate::BodyBuilder::frame`] refuses what [`crate::CheckView`] refuses: no check, a `session` of all zero, and a
/// session at or below the one before.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Check {
    checks: Vec<SessionCheck>,
}

impl Check {
    /// A Check of `checks`, a request's or a reply's.
    #[must_use]
    pub fn new(checks: Vec<SessionCheck>) -> Self {
        Self { checks }
    }

    /// The checks, in order.
    #[must_use]
    pub fn checks(&self) -> &[SessionCheck] {
        &self.checks
    }
}
