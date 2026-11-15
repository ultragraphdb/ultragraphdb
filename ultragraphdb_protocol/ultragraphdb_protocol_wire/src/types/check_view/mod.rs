/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod check_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::check_kind::CheckKind;
use crate::types::check_outcome::CheckOutcome;
use crate::types::command::Command;
use crate::types::session_check::SessionCheck;
use crate::utils::fields::identity;
use crate::utils::le::bytes_at;

/// The size of one check.
pub(crate) const CHECK_SIZE: usize = 32;

/// The body of a received Check frame, read in place; see [`crate::Check`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckView<'a> {
    pub(crate) checks: &'a [[u8; CHECK_SIZE]],
}

/// A check's fields as its 32 bytes hold them, `kind` and `outcome` by their numbers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CheckRecord {
    pub(crate) session: [u8; 16],
    pub(crate) principal: u64,
    pub(crate) kind: u8,
    pub(crate) outcome: u8,
}

impl CheckRecord {
    /// The fields of `record`.
    pub(crate) fn of(record: &[u8; CHECK_SIZE]) -> Self {
        Self {
            session: identity(record, 0),
            principal: bytes_at(record, 16).map_or(0, u64::from_le_bytes),
            kind: record[24],
            outcome: record[25],
        }
    }

    /// The fields as a check; `None` for a `kind` or an `outcome` outside its set, which no checked record holds.
    fn check(self) -> Option<SessionCheck> {
        let kind = CheckKind::from_number(self.kind)?;
        let outcome = CheckOutcome::from_number(self.outcome)?;
        Some(SessionCheck::new(self.session, self.principal, kind).with_outcome(outcome))
    }
}

/// The refusal naming check `index`: [`ProtocolError::field_value`] (`"check"`, the index).
pub(crate) fn refused(index: usize) -> ProtocolError {
    // Lossless: a `usize` is at most 64 bits.
    ProtocolError::field_value(Command::Check.number(), "check", index as u64)
}

impl CheckView<'_> {
    /// The number of checks, at least one.
    #[must_use]
    pub fn len(&self) -> usize {
        self.checks.len()
    }

    /// Whether the frame holds no check, which no view does.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    /// Check `index`; `None` past the last.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<SessionCheck> {
        CheckRecord::of(self.checks.get(index)?).check()
    }

    /// The body as the request the system group receives: every `outcome` 0.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] (`"check"`, the check's index) for the first check holding an outcome.
    pub fn check_request(&self) -> Result<(), ProtocolError> {
        let answered = self
            .checks
            .iter()
            .position(|record| CheckRecord::of(record).outcome != 0);
        answered.map_or(Ok(()), |index| Err(refused(index)))
    }

    /// The body as the reply to the request of `sent`, which the requester receives: as many checks as it sent, each
    /// with an outcome and the `session`, `principal` and `kind` sent at its index.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::count_mismatch`] (`"checks"`, the reply's count against the request's) for another number of
    /// checks, before any check is compared; then [`ProtocolError::field_value`] (`"check"`, the check's index) for
    /// the first check holding an `outcome` of 0 or differing from the one sent.
    pub fn check_reply(&self, sent: &[SessionCheck]) -> Result<(), ProtocolError> {
        if self.checks.len() != sent.len() {
            return Err(ProtocolError::count_mismatch(
                Command::Check.number(),
                "checks",
                // The casts are lossless: a `usize` is at most 64 bits.
                self.checks.len() as u64,
                sent.len() as u64,
            ));
        }
        let other = self.checks.iter().zip(sent).position(|(record, sent)| {
            let got = CheckRecord::of(record);
            got.outcome == 0
                || got.session != sent.session()
                || got.principal != sent.principal()
                || got.kind != sent.kind().number()
        });
        other.map_or(Ok(()), |index| Err(refused(index)))
    }
}
