/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod toml_kind_display;

/// The kind of a value the subset holds; its text names it with its article, as an error does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TomlKind {
    /// A basic string.
    String,
    /// A decimal integer that fits in an `i64`.
    Integer,
    /// `true` or `false`.
    Boolean,
}
