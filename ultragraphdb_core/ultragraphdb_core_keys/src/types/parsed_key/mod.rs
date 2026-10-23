/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod parsed_key_display;

use crate::errors::key_error::KeyError;
use crate::keys::key_builders::generate_key;
use crate::keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_DATA, BYTE_INDEX, BYTE_REVERSE, BYTE_SCHEMA, BYTE_SPLIT,
    BYTE_TYPE, DEFAULT_PREFIX,
};

/// A key parsed into its parts, as [`parse`](crate::parse) returns it. The default is Go's zero `ParsedKey`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedKey {
    pub(crate) attr: String,
    pub(crate) uid: u64,
    pub(crate) start_uid: u64,
    pub(crate) term: Vec<u8>,
    pub(crate) count: u32,
    pub(crate) byte_type: u8,
    pub(crate) has_start_uid: bool,
    pub(crate) byte_prefix: u8,
}

impl ParsedKey {
    /// The namespaced attribute.
    pub fn attr(&self) -> &str {
        &self.attr
    }

    /// The uid of a data or reverse key.
    pub fn uid(&self) -> u64 {
        self.uid
    }

    /// The start uid of a split key.
    pub fn start_uid(&self) -> u64 {
        self.start_uid
    }

    /// The term of an index key.
    pub fn term(&self) -> &[u8] {
        &self.term
    }

    /// The count of a count or reverse count key.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// The data type byte following the attribute.
    pub fn byte_type(&self) -> u8 {
        self.byte_type
    }

    /// Whether the key is a split key, carrying a start uid.
    pub fn has_start_uid(&self) -> bool {
        self.has_start_uid
    }

    fn has_default_or_split_prefix(&self) -> bool {
        self.byte_prefix == DEFAULT_PREFIX || self.byte_prefix == BYTE_SPLIT
    }

    /// Whether the key is a data key.
    pub fn is_data(&self) -> bool {
        self.has_default_or_split_prefix() && self.byte_type == BYTE_DATA
    }

    /// Whether the key is a reverse key.
    pub fn is_reverse(&self) -> bool {
        self.has_default_or_split_prefix() && self.byte_type == BYTE_REVERSE
    }

    /// Whether the key is a count or a reverse count key.
    pub fn is_count_or_count_rev(&self) -> bool {
        self.is_count() || self.is_count_rev()
    }

    /// Whether the key is a count key.
    pub fn is_count(&self) -> bool {
        self.has_default_or_split_prefix() && self.byte_type == BYTE_COUNT
    }

    /// Whether the key is a reverse count key.
    pub fn is_count_rev(&self) -> bool {
        self.has_default_or_split_prefix() && self.byte_type == BYTE_COUNT_REV
    }

    /// Whether the key is an index key.
    pub fn is_index(&self) -> bool {
        self.has_default_or_split_prefix() && self.byte_type == BYTE_INDEX
    }

    /// Whether the key is a schema key.
    pub fn is_schema(&self) -> bool {
        self.byte_prefix == BYTE_SCHEMA
    }

    /// Whether the key is a type key.
    pub fn is_type(&self) -> bool {
        self.byte_prefix == BYTE_TYPE
    }

    /// Whether the key is of the data type `typ`; [`BYTE_COUNT`] and [`BYTE_COUNT_REV`] both mean either count key.
    pub fn is_of_type(&self, typ: u8) -> bool {
        match typ {
            BYTE_COUNT | BYTE_COUNT_REV => self.is_count_or_count_rev(),
            BYTE_REVERSE => self.is_reverse(),
            BYTE_INDEX => self.is_index(),
            BYTE_DATA => self.is_data(),
            _ => false,
        }
    }

    /// The first key after the keys of this key's predicate. Useful when iterating in the reverse order.
    ///
    /// # Errors
    ///
    /// As every key builder: on a key of the [`BYTE_UNUSED`](crate::BYTE_UNUSED) prefix, whose attribute is empty,
    /// where Dgraph panics, and on an attribute of `u16::MAX` bytes. The other prefix methods alike.
    pub fn skip_predicate(&self) -> Result<Vec<u8>, KeyError> {
        self.prefix_with(self.byte_prefix, 0xFF)
    }

    // TODO(Naman): Remove these functions as they are unused.

    /// The prefix of the data keys of this key's predicate, under this key's prefix.
    pub fn data_prefix(&self) -> Result<Vec<u8>, KeyError> {
        self.prefix_with(self.byte_prefix, BYTE_DATA)
    }

    /// The prefix of the index keys of this key's predicate.
    pub fn index_prefix(&self) -> Result<Vec<u8>, KeyError> {
        self.prefix_with(DEFAULT_PREFIX, BYTE_INDEX)
    }

    /// The prefix of the reverse keys of this key's predicate.
    pub fn reverse_prefix(&self) -> Result<Vec<u8>, KeyError> {
        self.prefix_with(DEFAULT_PREFIX, BYTE_REVERSE)
    }

    /// The prefix of the count keys, or with `reverse` the reverse count keys, of this key's predicate.
    pub fn count_prefix(&self, reverse: bool) -> Result<Vec<u8>, KeyError> {
        self.prefix_with(
            DEFAULT_PREFIX,
            if reverse { BYTE_COUNT_REV } else { BYTE_COUNT },
        )
    }

    fn prefix_with(&self, prefix: u8, last: u8) -> Result<Vec<u8>, KeyError> {
        let mut buf = generate_key(prefix, &self.attr, 1)?;
        buf.push(last);
        Ok(buf)
    }
}
