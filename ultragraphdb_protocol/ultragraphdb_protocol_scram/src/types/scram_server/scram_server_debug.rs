/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::scram_server::ScramServer;

impl fmt::Debug for ScramServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScramServer")
            .field("secret", &format_args!("<redacted>"))
            .field("iterations", &self.iterations)
            .field("relay", &self.relay)
            .field("held", &self.held)
            .finish()
    }
}
