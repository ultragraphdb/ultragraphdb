/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod invariant_error_display;
mod invariant_error_error;

/// An invariant of UltraGraphDB found violated: a bug, never an input. Each crate converts it into its own error's
/// `Invariant` variant, so a caller's `?` carries it up to the component that stops.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvariantError(pub InvariantErrorEnum);

/// The classification of an [`InvariantError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvariantErrorEnum {
    /// The invariant `name` does not hold. The name states the invariant as it should hold, such as "rounds are
    /// appended in position order", so a log line or a test failure reads as the property broken.
    Violated { name: &'static str },
}

impl InvariantError {
    /// The invariant `name` does not hold.
    #[expect(
        non_snake_case,
        reason = "error constructors are named for their variant (AGENTS.md, Errors)"
    )]
    pub fn Violated(name: &'static str) -> Self {
        Self(InvariantErrorEnum::Violated { name })
    }
}
