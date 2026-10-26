/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `x` (pure parts): key encoding, namespaced attributes, reserved and pre-defined predicates.
//!
//! A key is `prefix | namespace (u64 BE) | attribute length (u16 BE) | attribute | suffix`, byte-identical to
//! Dgraph's `x/keys.go`. Every key builder takes a namespaced attribute, `hex(namespace)-predicate`, as
//! [`namespace_attr`] forms it.

mod errors;
mod keys;
mod types;
mod values;
mod x;

pub use crate::errors::key_error::{KeyError, KeyErrorEnum};
pub use crate::errors::quad_key_error::{QuadKeyError, QuadKeyErrorEnum};
pub use crate::keys::key_builders::{
    count_key, data_key, data_prefix, index_key, index_key_after_all_terms, predicate_prefix,
    reverse_key, schema_key, schema_prefix, split_key, type_key, type_prefix,
};
pub use crate::keys::namespace_attr::{
    attr_in_root_namespace, is_reverse_attr, namespace_attr, namespace_attr_list,
    namespace_to_bytes, parse_attr, parse_attr_list, parse_namespace, parse_namespace_attr,
    parse_namespace_bytes,
};
pub use crate::keys::parse::parse;
pub use crate::keys::reserved_namespace::{
    is_registered_reserved_predicate, is_registered_reserved_type, register_reserved_namespace,
    reserved_predicate_value_lock,
};
pub use crate::keys::reserved_predicates::{
    all_acl_predicates, is_acl_predicate, is_internal_predicate, is_other_reserved_predicate,
    is_pre_defined_predicate, is_pre_defined_type, is_reserved_predicate, is_reserved_type,
    star_all_predicates,
};
pub use crate::keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_DATA, BYTE_INDEX, BYTE_REVERSE, BYTE_SCHEMA, BYTE_SPLIT,
    BYTE_TYPE, BYTE_UNUSED, DEFAULT_PREFIX, IGNORE_BYTES, NAMESPACE_OFFSET, NS_SEPARATOR,
    ROOT_NAMESPACE,
};
pub use crate::types::count_direction::CountDirection;
pub use crate::types::keyspace::{Keyspace, SPLIT_FLAG};
pub use crate::types::parsed_key::ParsedKey;
pub use crate::types::quad_key::{DEFAULT_GRAPH, QuadKey, key_prefix};
pub use crate::types::region::Region;
pub use crate::types::reserved_namespace::ReservedNamespace;
pub use crate::types::trust_marker::TrustMarker;
pub use crate::types::value_type_info::ValueTypeInfo;
pub use crate::values::value_type;
pub use crate::x::{FACET_DELIMITER, STAR, predicate_lang, remove_duplicates, to_hex};
