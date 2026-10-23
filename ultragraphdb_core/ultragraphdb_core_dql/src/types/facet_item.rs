/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// One item of a facet list, `[{orderdesc|orderasc|alias}:] [varname as] name`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FacetItem {
    pub name: String,
    pub alias: String,
    pub var_name: String,
    pub ordered: bool,
    pub orderdesc: bool,
}
