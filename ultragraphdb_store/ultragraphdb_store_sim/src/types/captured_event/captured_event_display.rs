/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::CapturedEvent;

/// `LEVEL target: message name=value ...`, the fields after the message in their order.
impl fmt::Display for CapturedEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.level,
            self.target,
            self.message().unwrap_or("")
        )?;
        self.fields
            .iter()
            .filter(|(name, _)| name != "message")
            .try_for_each(|(name, value)| write!(f, " {name}={value}"))
    }
}
