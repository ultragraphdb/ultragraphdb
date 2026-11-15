/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What the system group decided for one check of a Check (`PROTOCOL.md` §13.24), in the order it decides: 12, then
/// 21, then 14.
///
/// | Number | Outcome |
/// |---|---|
/// | 0 | Unanswered: the value a request holds |
/// | 1 | Passed |
/// | 12 | NotHeld: the registry does not hold the session |
/// | 14 | InFlight: the session has a request in flight, of either kind |
/// | 21 | OtherPrincipal: the session belongs to another principal |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckOutcome {
    /// The value a request holds: no outcome yet.
    Unanswered,
    /// The registry holds the session for the principal, and no request of it is in flight.
    Passed,
    /// The registry does not hold the session: code 12.
    NotHeld,
    /// The session has a request in flight: code 14.
    InFlight,
    /// The session belongs to another principal: code 21.
    OtherPrincipal,
}

impl CheckOutcome {
    /// Every outcome, in the order of its number.
    pub const ALL: [CheckOutcome; 5] = [
        CheckOutcome::Unanswered,
        CheckOutcome::Passed,
        CheckOutcome::NotHeld,
        CheckOutcome::InFlight,
        CheckOutcome::OtherPrincipal,
    ];

    /// The outcome's number: 0, 1, 12, 14 or 21.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            CheckOutcome::Unanswered => 0,
            CheckOutcome::Passed => 1,
            CheckOutcome::NotHeld => 12,
            CheckOutcome::InFlight => 14,
            CheckOutcome::OtherPrincipal => 21,
        }
    }

    /// The outcome numbered `number`; `None` for a number no outcome has.
    #[must_use]
    pub fn from_number(number: u8) -> Option<CheckOutcome> {
        CheckOutcome::ALL
            .into_iter()
            .find(|outcome| outcome.number() == number)
    }
}
