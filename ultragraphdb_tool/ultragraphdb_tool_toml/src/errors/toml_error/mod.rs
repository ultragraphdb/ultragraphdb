/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod toml_error_display;
mod toml_error_error;

use crate::types::toml_feature::TomlFeature;
use crate::types::toml_invalid::TomlInvalid;
use crate::types::toml_kind::TomlKind;
use crate::types::toml_limit::TomlLimit;

/// Why a settings file was refused, grouped by who refused it.
///
/// - **The parser:** [`Unsupported`](Self::Unsupported), a feature of TOML outside the subset the settings use;
///   [`Invalid`](Self::Invalid), a document TOML 1.0 itself refuses; [`Limit`](Self::Limit), a document past one of
///   the parser's bounds. Nothing of the document is kept.
/// - **The typed reads:** [`Missing`](Self::Missing) and [`WrongKind`](Self::WrongKind), a setting a role reads that
///   is absent or holds another kind of value; [`UnreadKey`](Self::UnreadKey), a setting no read took, such as a
///   misspelled one.
///
/// Lines count from 1. A table is named by its header; the root table, which holds the keys before any header, has
/// the empty name. The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TomlErrorEnum {
    /// Line `line` uses `feature`, which the subset leaves out.
    Unsupported {
        /// The feature outside the subset.
        feature: TomlFeature,
        /// The line it is on.
        line: usize,
    },
    /// Line `line` breaks a rule of TOML 1.0, for `reason`.
    Invalid {
        /// The rule broken.
        reason: TomlInvalid,
        /// The line that breaks it.
        line: usize,
    },
    /// The document passes `limit`, whose bound is `max`.
    Limit {
        /// The limit passed.
        limit: TomlLimit,
        /// Its bound: the most bytes, tables or keys it allows.
        max: usize,
    },
    /// The setting `key` of table `table`, read as `expected`, is not in the document.
    Missing {
        /// The table read, empty for the root table.
        table: String,
        /// The key read.
        key: String,
        /// The kind the read expected.
        expected: TomlKind,
    },
    /// The setting `key` of table `table`, read as `expected`, holds a value of kind `found`.
    WrongKind {
        /// The table read, empty for the root table.
        table: String,
        /// The key read.
        key: String,
        /// The kind the read expected.
        expected: TomlKind,
        /// The kind the document holds.
        found: TomlKind,
    },
    /// The setting `key` of table `table` was read by no one: a setting the role does not know.
    UnreadKey {
        /// The table holding it, empty for the root table.
        table: String,
        /// The key.
        key: String,
    },
}

/// Why a settings file was refused; see [`TomlErrorEnum`] for each variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TomlError(pub TomlErrorEnum);

impl TomlError {
    /// Line `line` uses `feature`, outside the subset.
    pub fn unsupported(feature: TomlFeature, line: usize) -> Self {
        Self(TomlErrorEnum::Unsupported { feature, line })
    }

    /// Line `line` breaks a rule of TOML 1.0, for `reason`.
    pub fn invalid(reason: TomlInvalid, line: usize) -> Self {
        Self(TomlErrorEnum::Invalid { reason, line })
    }

    /// The document passes `limit`, whose bound is `max`.
    pub fn limit(limit: TomlLimit, max: usize) -> Self {
        Self(TomlErrorEnum::Limit { limit, max })
    }

    /// The setting `key` of table `table`, read as `expected`, is not in the document.
    pub fn missing(table: impl Into<String>, key: impl Into<String>, expected: TomlKind) -> Self {
        Self(TomlErrorEnum::Missing {
            table: table.into(),
            key: key.into(),
            expected,
        })
    }

    /// The setting `key` of table `table`, read as `expected`, holds a value of kind `found`.
    pub fn wrong_kind(
        table: impl Into<String>,
        key: impl Into<String>,
        expected: TomlKind,
        found: TomlKind,
    ) -> Self {
        Self(TomlErrorEnum::WrongKind {
            table: table.into(),
            key: key.into(),
            expected,
            found,
        })
    }

    /// The setting `key` of table `table` was read by no one.
    pub fn unread_key(table: impl Into<String>, key: impl Into<String>) -> Self {
        Self(TomlErrorEnum::UnreadKey {
            table: table.into(),
            key: key.into(),
        })
    }
}
