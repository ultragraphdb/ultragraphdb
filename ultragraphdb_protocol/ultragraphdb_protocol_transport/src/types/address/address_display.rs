/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::Address;

impl fmt::Display for Address {
    /// The address as [`Address::parse`] takes it: `tcp://host:port` or `unix:///path`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.prefix(), self.target())
    }
}
