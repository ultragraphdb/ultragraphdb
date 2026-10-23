/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// An attribute of the `@groupby` directive, with its alias and language list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupByAttr {
    pub(crate) attr: String,
    pub(crate) alias: String,
    pub(crate) langs: Vec<String>,
}

impl GroupByAttr {
    pub fn attr(&self) -> &str {
        &self.attr
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn langs(&self) -> &[String] {
        &self.langs
    }
}
