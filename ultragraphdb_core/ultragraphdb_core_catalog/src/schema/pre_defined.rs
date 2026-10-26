/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Updates of the pre-defined predicates and types, checked against the initial ones.

use ultragraphdb_core_keys::{
    is_pre_defined_predicate, is_pre_defined_type, namespace_attr, parse_namespace,
};
use ultragraphdb_proto_pb::pb;

use crate::errors::schema_error::SchemaError;
use crate::schema::initial::{complete_initial_schema, complete_initial_types};

/// Dgraph's `IsUniqueDgraphXid`: whether `dgraph.xid` carries `@unique`.
pub const IS_UNIQUE_DGRAPH_XID: bool = true;

/// Dgraph's `CheckAndModifyPreDefPredicate`: whether the update of a pre-defined predicate differs from its
/// initial schema; `false` for any other predicate. An update of `dgraph.xid` that differs only by lacking
/// `@unique` is given it and accepted, which upgrades a schema of a version that did not apply it.
///
/// # Errors
///
/// The [`KeyError`](ultragraphdb_core_keys::KeyError) of a predicate that is not a namespaced attribute, on which
/// Dgraph panics; [`is_pre_def_type_changed`] alike for a type name.
pub fn check_and_modify_pre_def_predicate(
    update: &mut pb::SchemaUpdate,
) -> Result<bool, SchemaError> {
    // Return false for non-pre-defined predicates.
    if !is_pre_defined_predicate(&update.predicate)? {
        return Ok(false);
    }
    let ns = parse_namespace(&update.predicate)?;
    let Some(original) = complete_initial_schema(ns)
        .into_iter()
        .find(|original| original.predicate == update.predicate)
    else {
        return Ok(true);
    };

    // For the dgraph.xid predicate, only the Unique field is allowed to be changed. Previously, the Unique
    // attribute was not applied to the dgraph.xid predicate. For users upgrading from a lower version, we will
    // set Unique to true.
    if update.predicate == namespace_attr(ns, "dgraph.xid")
        && !update.unique
        && is_dgraph_xid_change_valid(&original, update)
    {
        update.unique = true;
        return Ok(false);
    }
    Ok(original != *update)
}

/// Whether the change of `dgraph.xid` is to `unique` alone.
fn is_dgraph_xid_change_valid(original: &pb::SchemaUpdate, update: &pb::SchemaUpdate) -> bool {
    compare_schema_updates(original, update) == ["Unique"]
}

/// Dgraph's `compareSchemaUpdates`: the Go names of the fields in which two updates differ, in the order of Go's
/// struct. Go's reflection tells a nil list from an empty one; the two are one here, and no update Dgraph
/// compares holds an empty non-nil list where the initial schema holds nil.
pub fn compare_schema_updates(
    original: &pb::SchemaUpdate,
    update: &pb::SchemaUpdate,
) -> Vec<&'static str> {
    let (o, u) = (original, update);
    [
        ("Predicate", o.predicate == u.predicate),
        ("ValueType", o.value_type == u.value_type),
        ("Directive", o.directive == u.directive),
        ("Tokenizer", o.tokenizer == u.tokenizer),
        ("Count", o.count == u.count),
        ("List", o.list == u.list),
        ("Upsert", o.upsert == u.upsert),
        ("Lang", o.lang == u.lang),
        ("Unique", o.unique == u.unique),
        ("NonNullable", o.non_nullable == u.non_nullable),
        (
            "NonNullableList",
            o.non_nullable_list == u.non_nullable_list,
        ),
        ("ObjectTypeName", o.object_type_name == u.object_type_name),
        ("NoConflict", o.no_conflict == u.no_conflict),
        ("IndexSpecs", o.index_specs == u.index_specs),
    ]
    .into_iter()
    .filter(|(_, same)| !same)
    .map(|(name, _)| name)
    .collect()
}

/// Dgraph's `IsPreDefTypeChanged`: whether the update of a pre-defined type differs from its initial definition
/// in its fields' predicates; `false` for any other type.
pub fn is_pre_def_type_changed(update: &pb::TypeUpdate) -> Result<bool, SchemaError> {
    // Return false for non-pre-defined types.
    if !is_pre_defined_type(&update.type_name)? {
        return Ok(false);
    }

    Ok(complete_initial_types(parse_namespace(&update.type_name)?)
        .iter()
        .filter(|original| original.type_name == update.type_name)
        .any(|original| {
            original.fields.len() != update.fields.len()
                || original
                    .fields
                    .iter()
                    .zip(&update.fields)
                    .any(|(field, updated)| field.predicate != updated.predicate)
        }))
}
