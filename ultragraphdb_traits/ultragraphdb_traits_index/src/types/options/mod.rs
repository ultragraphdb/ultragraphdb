/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/options.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;

use ultragraphdb_error_options::OptionsError;

use crate::traits::option_type::OptionType;
use crate::types::option_value::OptionValue;

/// Dgraph's `Options`: the named options an index is built with; values of different types may mix.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options(HashMap<String, OptionValue>);

impl Options {
    /// Dgraph's `NewOptions`: no options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Dgraph's `SetOpt`: sets `which` to `value`.
    pub fn set_opt(&mut self, which: impl Into<String>, value: OptionValue) -> &mut Self {
        self.0.insert(which.into(), value);
        self
    }

    /// Dgraph's `GetOpt`: `Ok(None)` when `which` is not set (Go returns the default and `false`), the value when
    /// it is of type `T`, and an error when it is of another (Go returns the default, `true` and the error).
    pub fn get_opt<T: OptionType>(&self, which: &str) -> Result<Option<T>, OptionsError> {
        match self.0.get(which) {
            None => Ok(None),
            Some(v) => T::from_option(v).map(Some).ok_or_else(|| {
                OptionsError::Type(format!(
                    "unable to assign a value of type {} to type {}",
                    v.go_type(),
                    T::GO_TYPE
                ))
            }),
        }
    }

    /// Dgraph's `GetInterfaceOpt`: the value of `which`, whatever its type.
    pub fn get_interface_opt(&self, which: &str) -> Option<&OptionValue> {
        self.0.get(which)
    }

    /// Dgraph's `Specifies`: whether `which` is set.
    pub fn specifies(&self, which: &str) -> bool {
        self.0.contains_key(which)
    }
}
