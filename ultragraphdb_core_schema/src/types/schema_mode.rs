/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Which schema of a predicate the state answers with, Dgraph's `IsWrite` context key.
///
/// While a background task builds a predicate's indexes, the state holds two schemas for it: mutations use the
/// new one, queries the old one less the tokenizers the new one drops.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SchemaMode {
    /// A query: the current schema. Go's context without `IsWrite`.
    #[default]
    Query,
    /// A mutation: the schema being applied in the background where there is one, else the current one. Go's
    /// `GetWriteContext`.
    Write,
}
