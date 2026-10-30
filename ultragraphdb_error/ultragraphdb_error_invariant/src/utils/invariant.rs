/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::invariant_error::InvariantError;

/// The check of the invariant `name`, which `holds` or not.
///
/// # Errors
///
/// [`InvariantError::Violated`] naming `name` when it does not hold, in a release build.
///
/// # Panics
///
/// In a debug build, when it does not hold: at the caller's location, with the message `invariant violated: {name}`,
/// so a test, a simulation seed or a fuzz input that breaks an invariant fails at once. A release build never panics.
#[track_caller]
pub fn invariant(holds: bool, name: &'static str) -> Result<(), InvariantError> {
    debug_assert!(holds, "invariant violated: {name}");
    if holds {
        Ok(())
    } else {
        Err(InvariantError::Violated(name))
    }
}
