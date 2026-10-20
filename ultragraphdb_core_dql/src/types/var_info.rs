/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A query parameter: the value the request gave and the type the query declared.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct VarInfo {
    pub value: String,
    pub typ: String,
}
