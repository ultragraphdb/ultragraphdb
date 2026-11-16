/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::challenge::Challenge;

impl fmt::Debug for Challenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Challenge")
            .field("name", &self.name)
            .field("iterations", &self.iterations)
            .field("salt", &format_args!("<{} bytes>", self.salt.len()))
            .finish()
    }
}
