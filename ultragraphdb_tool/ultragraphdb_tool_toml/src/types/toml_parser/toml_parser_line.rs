/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::toml_error::TomlError;
use crate::types::toml_entry::TomlEntry;
use crate::types::toml_feature::TomlFeature;
use crate::types::toml_invalid::TomlInvalid;
use crate::types::toml_parser::TomlParser;
use crate::utils::chars::{
    comment, end_of_line, is_bare, is_forbidden_control, skip_whitespace, split_run,
};
use crate::utils::value::value;

impl<'a> TomlParser<'a> {
    /// Reads line `line`, `text`, without its line ending: blank, a comment, a header or a key and its value.
    pub(super) fn line(&mut self, text: &'a str, line: usize) -> Result<(), TomlError> {
        let rest = skip_whitespace(text);
        let mut chars = rest.chars();
        match chars.next() {
            None => Ok(()),
            Some('#') => comment(rest, line),
            Some('[') => self.header(chars.as_str(), line),
            Some(c) if is_forbidden_control(c) => {
                Err(TomlError::invalid(TomlInvalid::ControlCharacter, line))
            }
            Some(_) => self.key_value(rest, line),
        }
    }

    /// Reads a table header, `inner` the rest of its line after the opening bracket.
    fn header(&mut self, inner: &'a str, line: usize) -> Result<(), TomlError> {
        if inner.starts_with('[') {
            return Err(TomlError::unsupported(TomlFeature::ArraysOfTables, line));
        }
        let inner = skip_whitespace(inner);
        if inner.starts_with(['"', '\'']) {
            return Err(TomlError::unsupported(TomlFeature::QuotedKeys, line));
        }
        let (name, after) = split_run(inner, is_bare);
        let malformed = || TomlError::invalid(TomlInvalid::MalformedHeader, line);
        if name.is_empty() {
            return Err(malformed());
        }
        let after = skip_whitespace(after);
        if after.starts_with('.') {
            return Err(TomlError::unsupported(TomlFeature::DottedKeys, line));
        }
        let after = after.strip_prefix(']').ok_or_else(malformed)?;
        end_of_line(after, line)?;
        self.define_table(name, line)
    }

    /// Reads a `key = value` line, `rest` the line from its key on.
    fn key_value(&mut self, rest: &'a str, line: usize) -> Result<(), TomlError> {
        if rest.starts_with(['"', '\'']) {
            return Err(TomlError::unsupported(TomlFeature::QuotedKeys, line));
        }
        let (key, after_key) = split_run(rest, is_bare);
        if key.is_empty() {
            return Err(TomlError::invalid(TomlInvalid::MalformedKey, line));
        }
        let mut chars = skip_whitespace(after_key).chars();
        match chars.next() {
            Some('=') => {}
            Some('.') => return Err(TomlError::unsupported(TomlFeature::DottedKeys, line)),
            None | Some('#') => {
                return Err(TomlError::invalid(TomlInvalid::KeyWithoutValue, line));
            }
            Some(_) => return Err(TomlError::invalid(TomlInvalid::MalformedKey, line)),
        }
        self.claim_key(key, line)?;
        let (parsed, after_value) = value(skip_whitespace(chars.as_str()), line)?;
        end_of_line(after_value, line)?;
        self.current.push(TomlEntry::new(key.to_string(), parsed));
        Ok(())
    }
}
