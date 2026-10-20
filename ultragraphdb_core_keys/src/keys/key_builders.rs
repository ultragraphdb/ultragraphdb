/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The key builders. Each takes a namespaced attribute and panics, as Dgraph does, on one that
//! [`parse_namespace_attr`](crate::parse_namespace_attr) rejects or whose predicate is `u16::MAX` bytes or longer.

use crate::errors::key_error::{KeyError, KeyErrorEnum};
use crate::keys::namespace_attr::parse_namespace_bytes;
use crate::keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_DATA, BYTE_INDEX, BYTE_REVERSE, BYTE_SCHEMA, BYTE_SPLIT,
    BYTE_TYPE, DEFAULT_PREFIX,
};

/// Writes the initial bytes of a key: the type byte, the namespace, the length of the attribute and the attribute
/// itself. `extra` is the capacity reserved for the suffix the caller appends.
pub(crate) fn generate_key(type_byte: u8, attr: &str, extra: usize) -> Vec<u8> {
    // Separate namespace and attribute from attr and write namespace in the first 8 bytes of key.
    let (namespace, attr) = parse_namespace_bytes(attr);
    assert!(
        attr.len() < usize::from(u16::MAX),
        "attribute of {} bytes is too long for a key",
        attr.len()
    );
    let prefix_len = 1 + 8 + 2 + attr.len(); // byteType + ns + len(pred) + pred
    let mut buf = Vec::with_capacity(prefix_len + extra);
    buf.push(type_byte);
    buf.extend_from_slice(&namespace);
    buf.extend_from_slice(&(attr.len() as u16).to_be_bytes());
    buf.extend_from_slice(attr.as_bytes());
    buf
}

/// The schema key of an attribute. Schema keys are stored separately with a unique prefix, since all of them are
/// iterated over.
///
/// The structure of a schema key is as follows:
///
/// byte 0: key type prefix (set to [`BYTE_SCHEMA`])
/// bytes 1-8: namespace
/// bytes 9-10: length of attr
/// next len(attr) bytes: value of attr
pub fn schema_key(attr: &str) -> Vec<u8> {
    generate_key(BYTE_SCHEMA, attr, 0)
}

/// The type key of a type name. Type keys are stored separately with a unique prefix, since all of them are
/// iterated over. Its structure is a schema key's, with the prefix [`BYTE_TYPE`].
pub fn type_key(attr: &str) -> Vec<u8> {
    generate_key(BYTE_TYPE, attr, 0)
}

/// The data key of an attribute and a uid.
///
/// The structure of a data key is as follows:
///
/// byte 0: key type prefix (set to [`DEFAULT_PREFIX`] or [`BYTE_SPLIT`] if part of a multi-part list)
/// bytes 1-8: namespace
/// bytes 9-10: length of attr
/// next len(attr) bytes: value of attr
/// next byte: data type prefix (set to [`BYTE_DATA`])
/// next eight bytes: value of uid
/// next eight bytes (optional): if the key corresponds to a split list, the startUid of
/// the split stored in this key and the first byte will be sets to [`BYTE_SPLIT`].
pub fn data_key(attr: &str, uid: u64) -> Vec<u8> {
    uid_key(BYTE_DATA, attr, uid)
}

/// The reverse key of an attribute and a uid. Its structure is a data key's, with the data type prefix
/// [`BYTE_REVERSE`].
pub fn reverse_key(attr: &str, uid: u64) -> Vec<u8> {
    uid_key(BYTE_REVERSE, attr, uid)
}

fn uid_key(byte_type: u8, attr: &str, uid: u64) -> Vec<u8> {
    let mut buf = generate_key(DEFAULT_PREFIX, attr, 1 + 8);
    buf.push(byte_type);
    buf.extend_from_slice(&uid.to_be_bytes());
    buf
}

/// The index key with the term `0xFF`, lexicographically after the index key of any real term of the attribute.
pub fn index_key_after_all_terms(attr: &str) -> Vec<u8> {
    index_key(attr, &[0xFF])
}

/// The index key of an attribute and a term.
///
/// The structure of an index key is as follows:
///
/// byte 0: key type prefix (set to [`DEFAULT_PREFIX`] or [`BYTE_SPLIT`] if part of a multi-part list)
/// bytes 1-8: namespace
/// bytes 9-10: length of attr
/// next len(attr) bytes: value of attr
/// next byte: data type prefix (set to [`BYTE_INDEX`])
/// next len(term) bytes: value of term
/// next eight bytes (optional): if the key corresponds to a split list, the startUid of
/// the split stored in this key.
pub fn index_key(attr: &str, term: &[u8]) -> Vec<u8> {
    let mut buf = generate_key(DEFAULT_PREFIX, attr, 1 + term.len());
    buf.push(BYTE_INDEX);
    buf.extend_from_slice(term);
    buf
}

/// The count key of an attribute and a count, or the reverse count key when `reverse` is set.
///
/// The structure of a count key is as follows:
///
/// byte 0: key type prefix (set to [`DEFAULT_PREFIX`])
/// bytes 1-8: namespace
/// bytes 9-10: length of attr
/// next len(attr) bytes: value of attr
/// next byte: data type prefix (set to [`BYTE_COUNT`] or [`BYTE_COUNT_REV`])
/// next four bytes: value of count.
pub fn count_key(attr: &str, count: u32, reverse: bool) -> Vec<u8> {
    let mut buf = generate_key(DEFAULT_PREFIX, attr, 1 + 4);
    buf.push(if reverse { BYTE_COUNT_REV } else { BYTE_COUNT });
    buf.extend_from_slice(&count.to_be_bytes());
    buf
}

/// The prefix of every schema key.
pub fn schema_prefix() -> Vec<u8> {
    vec![BYTE_SCHEMA]
}

/// The prefix of every type key.
pub fn type_prefix() -> Vec<u8> {
    vec![BYTE_TYPE]
}

/// The prefix of every key of this predicate except its schema key.
pub fn predicate_prefix(predicate: &str) -> Vec<u8> {
    generate_key(DEFAULT_PREFIX, predicate, 0)
}

/// The prefix of every data key of a namespace.
pub fn data_prefix(ns: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 8);
    buf.push(DEFAULT_PREFIX);
    buf.extend_from_slice(&ns.to_be_bytes());
    buf
}

/// The key of the part of a split list that starts at `start_uid`: `base_key` with its first byte set to
/// [`BYTE_SPLIT`] and the start uid appended.
///
/// # Errors
///
/// When `base_key` does not have the default prefix.
pub fn split_key(base_key: &[u8], start_uid: u64) -> Result<Vec<u8>, KeyError> {
    let mut key_copy = Vec::with_capacity(base_key.len() + 8);
    key_copy.extend_from_slice(base_key);
    key_copy.extend_from_slice(&[0; 8]);

    // As in Dgraph, an empty base key reads the first byte of the start uid's zeroed space here.
    if key_copy[0] != DEFAULT_PREFIX {
        return Err(KeyError::new(KeyErrorEnum::SplitNonDefaultPrefix));
    }
    // Change the first byte (i.e the key prefix) to BYTE_SPLIT to signal this is an
    // individual part of a single list key.
    key_copy[0] = BYTE_SPLIT;

    // Append the start uid at the end of the key.
    key_copy[base_key.len()..].copy_from_slice(&start_uid.to_be_bytes());
    Ok(key_copy)
}
