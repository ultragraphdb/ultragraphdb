/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::toml_kind::TomlKind;

/// A value of the subset: a basic string with its escapes resolved, a decimal integer, or a boolean.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TomlValue {
    /// A basic string, its escapes resolved.
    String(String),
    /// A decimal integer.
    Integer(i64),
    /// `true` or `false`.
    Boolean(bool),
}

impl TomlValue {
    /// The value's kind.
    #[must_use]
    pub fn kind(&self) -> TomlKind {
        match self {
            Self::String(_) => TomlKind::String,
            Self::Integer(_) => TomlKind::Integer,
            Self::Boolean(_) => TomlKind::Boolean,
        }
    }
}
