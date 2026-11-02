/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod toml_limit_display;

/// A bound of the parser, which a document past it is refused for; the error carries the bound's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TomlLimit {
    /// The document's size in bytes.
    DocumentBytes,
    /// A line's size in bytes, without its line ending.
    LineBytes,
    /// The number of tables with a header; the root table is not counted.
    Tables,
    /// The number of keys, in every table together.
    Keys,
}
