/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/allowed_options.go (OptionValuePair). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// An option named with the text of its value, as a schema states it (`metric: "cosine"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionValuePair {
    option: String,
    value: String,
}

impl OptionValuePair {
    /// The pair of `option` and the text `value`.
    pub fn new(option: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            option: option.into(),
            value: value.into(),
        }
    }

    /// The option's name.
    pub fn option(&self) -> &str {
        &self.option
    }

    /// The text of its value.
    pub fn value(&self) -> &str {
        &self.value
    }
}
