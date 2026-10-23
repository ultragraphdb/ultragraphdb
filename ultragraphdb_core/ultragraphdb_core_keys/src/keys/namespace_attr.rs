/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Namespaced attribute names: an attribute held in memory is `hex(namespace)-predicate`.
//!
//! Like Dgraph, the parsers expect a well-formed name and panic on one without a separator or with a namespace that
//! is not hexadecimal; Dgraph builds every such name itself.

use ultragraphdb_common_utils::go_strconv;

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
/// # Panics
///
/// When `attr` has no separator or its namespace is not hexadecimal.
pub fn parse_namespace_attr(attr: &str) -> (u64, &str) {
    let (ns, pred) = split(attr);
    (str_to_uint(ns), pred)
}

/// The namespace, as a key's eight bytes, and the predicate of a namespaced attribute.
///
/// # Panics
///
/// As [`parse_namespace_attr`].
pub fn parse_namespace_bytes(attr: &str) -> ([u8; 8], &str) {
    let (ns, pred) = parse_namespace_attr(attr);
    (namespace_to_bytes(ns), pred)
}

/// The predicate of a namespaced attribute.
///
/// # Panics
///
/// When `attr` has no separator.
pub fn parse_attr(attr: &str) -> &str {
    split(attr).1
}

/// The namespace of a namespaced attribute; a name without a separator is read whole as the namespace.
///
/// # Panics
///
/// When the namespace is not hexadecimal.
pub fn parse_namespace(attr: &str) -> u64 {
    str_to_uint(attr.split_once(NS_SEPARATOR).map_or(attr, |(ns, _)| ns))
}

/// [`parse_attr`] of every attribute.
pub fn parse_attr_list(attrs: &[String]) -> Vec<String> {
    attrs
        .iter()
        .map(|attr| parse_attr(attr).to_string())
        .collect()
}

/// Whether the predicate of a namespaced attribute is a reverse one, `~name`.
///
/// # Panics
///
/// When `attr` has no separator or its predicate is empty.
pub fn is_reverse_attr(attr: &str) -> bool {
    parse_attr(attr).as_bytes()[0] == b'~'
}

fn split(attr: &str) -> (&str, &str) {
    attr.split_once(NS_SEPARATOR)
        .unwrap_or_else(|| panic!("attribute {attr:?} has no namespace separator {NS_SEPARATOR:?}"))
}

// For consistency, use base16 to encode/decode the namespace.
fn str_to_uint(s: &str) -> u64 {
    go_strconv::parse_uint(s, 16)
        .unwrap_or_else(|_| panic!("namespace {s:?} is not a hexadecimal u64"))
}

fn uint_to_str(ns: u64) -> String {
    format!("{ns:x}")
}
