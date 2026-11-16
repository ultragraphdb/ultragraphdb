/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::scram_verifier::ScramVerifier;

impl fmt::Debug for ScramVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScramVerifier")
            .field("salt", &self.salt)
            .field("iterations", &self.iterations)
            .field("stored_key", &format_args!("<redacted>"))
            .field("server_key", &format_args!("<redacted>"))
            .finish()
    }
}
