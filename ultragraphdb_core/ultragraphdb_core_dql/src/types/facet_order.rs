/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Ordering by one facet key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FacetOrder {
    pub(crate) key: String,
    pub(crate) desc: bool,
}

impl FacetOrder {
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Whether the order is descending by this facet.
    pub fn desc(&self) -> bool {
        self.desc
    }
}
