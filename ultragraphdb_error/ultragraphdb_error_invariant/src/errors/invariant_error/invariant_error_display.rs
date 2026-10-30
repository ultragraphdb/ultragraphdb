/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::{InvariantError, InvariantErrorEnum};

impl fmt::Display for InvariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            InvariantErrorEnum::Violated { name } => write!(f, "invariant violated: {name}"),
        }
    }
}
