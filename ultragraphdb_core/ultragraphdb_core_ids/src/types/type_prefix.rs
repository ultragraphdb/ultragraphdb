/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU16;

use crate::errors::id_error::IdError;

/// The 16-bit type prefix of a [`SystemId`](crate::SystemId), the top 16 bits of the ID. Prefix 0 is invalid; 1 is
/// the built-in `Resource` type and 2 the built-in `Statement` type, reserved before any declared type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypePrefix(NonZeroU16);

impl TypePrefix {
    /// The built-in `Resource` type, whose key is an IRI: prefix 1.
    pub const RESOURCE: Self = Self(NonZeroU16::MIN);
    /// The built-in `Statement` type: prefix 2.
    pub const STATEMENT: Self = Self(NonZeroU16::MIN.saturating_add(1));

    /// The type prefix `prefix`.
    ///
    /// # Errors
    ///
    /// [`IdError::zero_type_prefix`] when `prefix` is 0.
    pub fn new(prefix: u16) -> Result<Self, IdError> {
        NonZeroU16::new(prefix)
            .map(Self)
            .ok_or_else(IdError::zero_type_prefix)
    }

    /// The prefix as a number.
    #[must_use]
    pub fn get(self) -> u16 {
        self.0.get()
    }
}
