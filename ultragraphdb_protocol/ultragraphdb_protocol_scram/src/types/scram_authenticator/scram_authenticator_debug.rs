/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::scram_authenticator::ScramAuthenticator;

impl<C: fmt::Debug, N: fmt::Debug> fmt::Debug for ScramAuthenticator<C, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScramAuthenticator")
            .field("credentials", &self.credentials)
            .field("nonces", &self.nonces)
            .field("secret", &format_args!("<redacted>"))
            .field("iterations", &self.iterations)
            .finish()
    }
}
