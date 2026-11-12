/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

// Arithmetic on decoded text input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

//! The settings files' TOML: an in-house parser of the subset of TOML 1.0 the roles' settings use, refusing every
//! other feature by name, and the typed reads a role takes its settings through.
//!
//! [`TomlDocument::parse`] reads a document in one pass, line by line, with no recursion: blank lines, comments,
//! `[table]` headers with a bare name, keys in the root table before any header, and `key = value` lines with a bare
//! key and a basic string, a decimal integer or a boolean. A document it accepts parses to what any TOML 1.0 parser
//! reads from it. [`TomlDocument::string`], [`TomlDocument::integer`] and [`TomlDocument::boolean`] read a setting by
//! table and key; [`TomlDocument::finish`] then refuses any key no read took, so a misspelled setting stops the start.
//! Every refusal is a [`TomlError`]: a feature outside the subset by name and line, a rule of TOML 1.0 broken, a limit
//! passed, a setting missing, of the wrong kind or left unread. The parser belongs to the control plane: a role reads
//! its settings once, at start.

mod errors;
mod types;
mod utils;

pub use crate::errors::toml_error::{TomlError, TomlErrorEnum};
pub use crate::types::toml_document::TomlDocument;
pub use crate::types::toml_entry::TomlEntry;
pub use crate::types::toml_feature::TomlFeature;
pub use crate::types::toml_invalid::TomlInvalid;
pub use crate::types::toml_kind::TomlKind;
pub use crate::types::toml_limit::TomlLimit;
pub use crate::types::toml_table::TomlTable;
pub use crate::types::toml_value::TomlValue;
