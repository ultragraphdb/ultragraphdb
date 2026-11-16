/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::scram_login::ScramLogin;

impl<N: fmt::Debug> fmt::Debug for ScramLogin<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScramLogin")
            .field("name", &self.name)
            .field("password", &format_args!("<redacted>"))
            .field("nonces", &self.nonces)
            .finish()
    }
}
