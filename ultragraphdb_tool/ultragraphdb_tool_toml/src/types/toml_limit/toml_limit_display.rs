/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::toml_limit::TomlLimit;

impl fmt::Display for TomlLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self {
            Self::DocumentBytes => "bytes",
            Self::LineBytes => "bytes per line",
            Self::Tables => "tables",
            Self::Keys => "keys",
        };
        f.write_str(unit)
    }
}
