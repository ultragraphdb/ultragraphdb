/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::verify::Verify;

impl fmt::Debug for Verify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Verify")
            .field("name", &self.name)
            .field(
                "auth_message",
                &format_args!("<{} bytes>", self.auth_message.len()),
            )
            .field("proof", &format_args!("<redacted>"))
            .field(
                "verified",
                &self
                    .verified
                    .map(|(principal, _)| (principal, format_args!("<redacted>"))),
            )
            .finish()
    }
}
