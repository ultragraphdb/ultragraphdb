/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::check_kind::CheckKind;
use crate::types::check_outcome::CheckOutcome;

/// One check of a Check (`PROTOCOL.md` §13.24), 32 bytes: whether the registry holds the session and the principal is
/// the one it belongs to.
///
/// | Check offset | Field | Type |
/// |---|---|---|
/// | 0 | `session` | 16 bytes, not all zero |
/// | 16 | `principal` | u64: the connection's principal |
/// | 24 | `kind` | u8: [`CheckKind`] |
/// | 25 | `outcome` | u8: [`CheckOutcome`], 0 in a request |
/// | 26 | zero | 6 bytes |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionCheck {
    session: [u8; 16],
    principal: u64,
    kind: CheckKind,
    outcome: CheckOutcome,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<SessionCheck>() <= 64);

impl SessionCheck {
    /// A request's check of `session` for `principal`, of `kind`, its outcome [`CheckOutcome::Unanswered`].
    #[must_use]
    pub fn new(session: [u8; 16], principal: u64, kind: CheckKind) -> Self {
        Self {
            session,
            principal,
            kind,
            outcome: CheckOutcome::Unanswered,
        }
    }

    /// The check with its `outcome`, as a reply holds it.
    #[must_use]
    pub fn with_outcome(self, outcome: CheckOutcome) -> Self {
        Self { outcome, ..self }
    }

    /// The session's identity.
    #[must_use]
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The connection's principal.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// What the check asks.
    #[must_use]
    pub fn kind(&self) -> CheckKind {
        self.kind
    }

    /// What the system group decided; [`CheckOutcome::Unanswered`] in a request.
    #[must_use]
    pub fn outcome(&self) -> CheckOutcome {
        self.outcome
    }
}
