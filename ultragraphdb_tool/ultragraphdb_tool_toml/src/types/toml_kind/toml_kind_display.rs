/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::toml_kind::TomlKind;

impl fmt::Display for TomlKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::String => "a string",
            Self::Integer => "an integer",
            Self::Boolean => "a boolean",
        };
        f.write_str(name)
    }
}
