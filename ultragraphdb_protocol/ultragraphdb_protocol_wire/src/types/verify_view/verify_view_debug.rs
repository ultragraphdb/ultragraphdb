/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::verify_view::VerifyView;

impl fmt::Debug for VerifyView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifyView")
            .field("name", &self.name)
            .field(
                "auth_message",
                &format_args!("<{} bytes>", self.auth_message.len()),
            )
            .field("proof", &format_args!("<redacted>"))
            .field("verified", &self.verified)
            .field("principal", &self.principal)
            .field(
                "signature",
                &self.signature.map(|_| format_args!("<redacted>")),
            )
            .finish()
    }
}
