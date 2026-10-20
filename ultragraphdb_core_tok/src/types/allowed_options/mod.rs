/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/allowed_options.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;

use crate::errors::options_error::{OptionsError, OptionsErrorEnum};
use crate::types::option_parser::OptionParser;
use crate::types::option_value::OptionValue;
use crate::types::option_value_pair::OptionValuePair;
use crate::types::options::Options;
use crate::utils::option_parser::{float64_opt_parser, int_opt_parser, string_opt_parser};

/// Dgraph's `AllowedOptions`: the options an index takes, each with the parser of its value's text.
#[derive(Debug, Clone, Default)]
pub struct AllowedOptions(HashMap<String, OptionParser>);

impl AllowedOptions {
    /// Dgraph's `NewAllowedOptions`: none allowed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Dgraph's `AddIntOption`.
    pub fn add_int_option(&mut self, name: impl Into<String>) -> &mut Self {
        self.add_custom_option(name, int_opt_parser)
    }

    /// Dgraph's `AddStringOption`.
    pub fn add_string_option(&mut self, name: impl Into<String>) -> &mut Self {
        self.add_custom_option(name, string_opt_parser)
    }

    /// Dgraph's `AddFloat64Option`.
    pub fn add_float64_option(&mut self, name: impl Into<String>) -> &mut Self {
        self.add_custom_option(name, float64_opt_parser)
    }

    /// Dgraph's `AddCustomOption`.
    pub fn add_custom_option(
        &mut self,
        name: impl Into<String>,
        parser: OptionParser,
    ) -> &mut Self {
        self.0.insert(name.into(), parser);
        self
    }

    /// Dgraph's `GetOptionList`: the allowed option names, sorted here. Go's loop never advances its index, so it
    /// returns one name and empty strings; nothing in Dgraph calls it.
    pub fn get_option_list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.0.keys().cloned().collect();
        names.sort();
        names
    }

    /// Dgraph's `GetParsedOption`: the value of `name` parsed from `value`, or `option <name> is not allowed!`.
    pub fn get_parsed_option(&self, name: &str, value: &str) -> Result<OptionValue, OptionsError> {
        let parser = self.0.get(name).ok_or_else(|| {
            OptionsError::new(OptionsErrorEnum::NotAllowed(format!(
                "option {name} is not allowed!"
            )))
        })?;
        parser(value)
    }

    /// Dgraph's `ParsePair`.
    pub fn parse_pair(&self, pair: &OptionValuePair) -> Result<OptionValue, OptionsError> {
        self.get_parsed_option(pair.option(), pair.value())
    }

    /// Dgraph's `PopulateOptions`: each pair parsed into `opts`, stopping at the first error.
    pub fn populate_options(
        &self,
        pairs: &[OptionValuePair],
        opts: &mut Options,
    ) -> Result<(), OptionsError> {
        for p in pairs {
            let v = self.parse_pair(p)?;
            opts.set_opt(p.option(), v);
        }
        Ok(())
    }
}
