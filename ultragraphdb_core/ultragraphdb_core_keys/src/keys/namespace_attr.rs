/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Namespaced attribute names: an attribute held in memory is `hex(namespace)-predicate`.
//!
//! Dgraph builds every such name itself, and its parsers panic on one without a separator or with a namespace that
//! is not hexadecimal; the port's refuse it with a [`KeyError`].

use ultragraphdb_common_utils::go_strconv;

use crate::errors::key_error::KeyError;
use crate::keys::{NS_SEPARATOR, ROOT_NAMESPACE};

/// The namespace as the eight big-endian bytes a key carries.
pub fn namespace_to_bytes(ns: u64) -> [u8; 8] {
    ns.to_be_bytes()
}

/// The attribute name of `attr` in namespace `ns`: `hex(ns)-attr`.
pub fn namespace_attr(ns: u64, attr: &str) -> String {
    format!("{}{NS_SEPARATOR}{attr}", uint_to_str(ns))
}

/// [`namespace_attr`] of every predicate.
pub fn namespace_attr_list(ns: u64, preds: &[String]) -> Vec<String> {
    preds.iter().map(|pred| namespace_attr(ns, pred)).collect()
}

/// The attribute name of `attr` in the root namespace.
pub fn attr_in_root_namespace(attr: &str) -> String {
    namespace_attr(ROOT_NAMESPACE, attr)
}

/// The namespace and the predicate of a namespaced attribute.
///
/// # Errors
///
/// [`MissingNamespaceSeparator`](crate::KeyErrorEnum::MissingNamespaceSeparator) when `attr` has no separator and
/// [`InvalidNamespace`](crate::KeyErrorEnum::InvalidNamespace) when its namespace is not hexadecimal; Dgraph
/// panics on both.
pub fn parse_namespace_attr(attr: &str) -> Result<(u64, &str), KeyError> {
    let (ns, pred) = split(attr)?;
    Ok((str_to_uint(attr, ns)?, pred))
}

/// The namespace, as a key's eight bytes, and the predicate of a namespaced attribute.
///
/// # Errors
///
/// As [`parse_namespace_attr`].
pub fn parse_namespace_bytes(attr: &str) -> Result<([u8; 8], &str), KeyError> {
    let (ns, pred) = parse_namespace_attr(attr)?;
    Ok((namespace_to_bytes(ns), pred))
}

/// The predicate of a namespaced attribute.
///
/// # Errors
///
/// [`MissingNamespaceSeparator`](crate::KeyErrorEnum::MissingNamespaceSeparator) when `attr` has no separator,
/// on which Dgraph panics.
pub fn parse_attr(attr: &str) -> Result<&str, KeyError> {
    Ok(split(attr)?.1)
}

/// The namespace of a namespaced attribute; a name without a separator is read whole as the namespace.
///
/// # Errors
///
/// [`InvalidNamespace`](crate::KeyErrorEnum::InvalidNamespace) when the namespace is not hexadecimal, on which
/// Dgraph panics.
pub fn parse_namespace(attr: &str) -> Result<u64, KeyError> {
    str_to_uint(
        attr,
        attr.split_once(NS_SEPARATOR).map_or(attr, |(ns, _)| ns),
    )
}

/// [`parse_attr`] of every attribute.
///
/// # Errors
///
/// As [`parse_attr`], for the first attribute it refuses.
pub fn parse_attr_list(attrs: &[String]) -> Result<Vec<String>, KeyError> {
    attrs
        .iter()
        .map(|attr| parse_attr(attr).map(str::to_string))
        .collect()
}

/// Whether the predicate of a namespaced attribute is a reverse one, `~name`. An empty predicate is not, where
/// Dgraph panics reading its first byte.
///
/// # Errors
///
/// As [`parse_attr`].
pub fn is_reverse_attr(attr: &str) -> Result<bool, KeyError> {
    Ok(parse_attr(attr)?.starts_with('~'))
}

fn split(attr: &str) -> Result<(&str, &str), KeyError> {
    attr.split_once(NS_SEPARATOR)
        .ok_or_else(|| KeyError::MissingNamespaceSeparator(attr.to_string()))
}

// For consistency, use base16 to encode/decode the namespace.
fn str_to_uint(attr: &str, s: &str) -> Result<u64, KeyError> {
    go_strconv::parse_uint(s, 16).map_err(|e| KeyError::InvalidNamespace(attr.to_string(), e))
}

fn uint_to_str(ns: u64) -> String {
    format!("{ns:x}")
}
