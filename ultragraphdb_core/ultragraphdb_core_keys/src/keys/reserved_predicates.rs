/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The reserved, pre-defined, ACL and internal predicate and type names. Checks that take a namespaced attribute
//! panic, as Dgraph's do, on a name without the namespace separator.

use ultragraphdb_common_utils::go_strings;

use crate::keys::namespace_attr::{namespace_attr, parse_attr};

// These predicates appear for queries that have * as predicate in them.
const STAR_ALL_PREDICATES: [&str; 1] = ["dgraph.type"];

const ACL_PREDICATES: [&str; 6] = [
    "dgraph.xid",
    "dgraph.password",
    "dgraph.user.group",
    "dgraph.rule.predicate",
    "dgraph.rule.permission",
    "dgraph.acl.rule",
];

// TODO: rename this map to a better suited name as per its properties. It is not just for GraphQL
// predicates, but for all those which are PreDefined and whose value is not allowed to be mutated
// by users. When renaming this also rename the IsGraphql context key in edgraph/server.go.
const OTHER_RESERVED_PREDICATES: [&str; 6] = [
    "dgraph.graphql.xid",
    "dgraph.graphql.schema",
    "dgraph.drop.op",
    "dgraph.graphql.p_query",
    "dgraph.namespace.id",
    "dgraph.namespace.name",
];

// An internal predicate is a predicate that has a special meaning in Dgraph and its query
// language and should not be allowed either as a user-defined predicate or as a
// predicate in initial internal schema.
const INTERNAL_PREDICATES: [&str; 1] = ["uid"];

const PRE_DEFINED_TYPES: [&str; 6] = [
    "dgraph.graphql",
    "dgraph.type.User",
    "dgraph.type.Group",
    "dgraph.type.Rule",
    "dgraph.graphql.persisted_query",
    "dgraph.namespace",
];

/// Whether the bare predicate is reserved by GraphQL and the like: a subset of the pre-defined predicates whose
/// value users may not mutate directly. Case-sensitive, as in Dgraph.
pub fn is_other_reserved_predicate(pred: &str) -> bool {
    OTHER_RESERVED_PREDICATES.contains(&pred)
}

/// Whether the namespaced predicate is reserved for internal usage, i.e., prefixed with `dgraph.` in any case.
///
/// `dgraph.` is reserved as the namespace for the types and predicates Dgraph may create in future, so users may
/// not create a predicate under it. Reserved predicates are a superset of pre-defined predicates; use
/// [`is_pre_defined_predicate`] to find out whether a predicate was actually defined internally. For example:
///
/// 1. `dgraph.type` (reserved = true,  pre_defined = true )
/// 2. `dgraph.blah` (reserved = true,  pre_defined = false)
/// 3. `person.name` (reserved = false, pre_defined = false)
pub fn is_reserved_predicate(pred: &str) -> bool {
    is_reserved_name(parse_attr(pred))
}

/// Whether the namespaced predicate is defined by Dgraph internally in the initial schema: `dgraph.type`, the ACL
/// predicates and the [other reserved](is_other_reserved_predicate) ones. These may not be dropped, nor may any
/// schema update differ from the initial internal schema for them.
pub fn is_pre_defined_predicate(pred: &str) -> bool {
    let pred = parse_attr(pred);
    STAR_ALL_PREDICATES.contains(&go_strings::to_lower(pred).as_str())
        || is_acl_predicate(pred)
        || is_other_reserved_predicate(pred)
}

/// Whether the bare predicate is one of the ACL feature's reserved predicates, in any case.
pub fn is_acl_predicate(pred: &str) -> bool {
    ACL_PREDICATES.contains(&go_strings::to_lower(pred).as_str())
}

/// The pre-defined predicates that `*` expands to, in the namespace.
pub fn star_all_predicates(namespace: u64) -> Vec<String> {
    STAR_ALL_PREDICATES
        .iter()
        .map(|pred| namespace_attr(namespace, pred))
        .collect()
}

/// The ACL predicates, bare. Dgraph returns them in map order; this port in declaration order.
pub fn all_acl_predicates() -> Vec<String> {
    ACL_PREDICATES.iter().map(|pred| pred.to_string()).collect()
}

/// Whether the namespaced predicate is internal, in any case. Currently, `uid` is the only such candidate.
pub fn is_internal_predicate(pred: &str) -> bool {
    INTERNAL_PREDICATES.contains(&go_strings::to_lower(parse_attr(pred)).as_str())
}

/// Whether the namespaced type is reserved for internal usage, i.e., prefixed with `dgraph.` in any case.
/// Pre-defined types are a subset of reserved types; see [`is_pre_defined_type`].
pub fn is_reserved_type(typ: &str) -> bool {
    is_reserved_name(parse_attr(typ))
}

/// Whether the namespaced type is defined by Dgraph internally, for example `dgraph.graphql` or the ACL types.
/// Case-sensitive, as in Dgraph.
pub fn is_pre_defined_type(typ: &str) -> bool {
    PRE_DEFINED_TYPES.contains(&parse_attr(typ))
}

// Whether the given name is prefixed with `dgraph.`.
fn is_reserved_name(name: &str) -> bool {
    go_strings::to_lower(name).starts_with("dgraph.")
}
