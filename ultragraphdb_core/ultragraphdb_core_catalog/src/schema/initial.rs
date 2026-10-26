/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go. Changed for ultragraphdb: translated to Rust; the ACL setting is a
 * parameter rather than the worker configuration.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The schema and types every namespace starts with.

use ultragraphdb_core_keys::{ROOT_NAMESPACE, namespace_attr};
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::posting::ValType;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

use crate::schema::pre_defined::IS_UNIQUE_DGRAPH_XID;

/// Dgraph's `InitialTypes`: the types to insert at the beginning of Dgraph's execution, the ACL ones when ACL is
/// enabled.
pub fn initial_types(namespace: u64, acl_enabled: bool) -> Vec<pb::TypeUpdate> {
    initial_types_internal(namespace, acl_enabled)
}

/// Dgraph's `CompleteInitialTypes`: all the initial types, whatever the ACL setting. Used where the setting is not
/// known in advance or every pre-defined type has to be considered, e.g. to let a type update through during alter
/// when it equals the pre-defined one, which live loading a previously exported schema needs.
pub fn complete_initial_types(namespace: u64) -> Vec<pb::TypeUpdate> {
    initial_types_internal(namespace, true)
}

/// Dgraph's `InitialSchema`: the predicates to insert at the beginning of Dgraph's execution, the ACL ones when
/// ACL is enabled.
pub fn initial_schema(namespace: u64, acl_enabled: bool) -> Vec<pb::SchemaUpdate> {
    initial_schema_internal(namespace, acl_enabled)
}

/// Dgraph's `CompleteInitialSchema`: all the initial predicates, whatever the ACL setting. Used where the setting
/// is not known in advance and it is better to create all the reserved predicates and remove them later than miss
/// some, e.g. during bulk loading.
pub fn complete_initial_schema(namespace: u64) -> Vec<pb::SchemaUpdate> {
    initial_schema_internal(namespace, true)
}

/// A schema update of a predicate, without its namespace yet.
fn su(predicate: &str, value_type: ValType) -> pb::SchemaUpdate {
    let mut u = pb::SchemaUpdate {
        predicate: predicate.to_string(),
        ..Default::default()
    };
    u.set_value_type(value_type);
    u
}

/// `su` indexed with one tokenizer.
fn indexed(predicate: &str, value_type: ValType, tokenizer: &str) -> pb::SchemaUpdate {
    let mut u = su(predicate, value_type);
    u.set_directive(Directive::Index);
    u.tokenizer = vec![tokenizer.to_string()];
    u
}

fn type_update(type_name: &str, fields: &[(&str, ValType)]) -> pb::TypeUpdate {
    pb::TypeUpdate {
        type_name: type_name.to_string(),
        fields: fields.iter().map(|(p, t)| su(p, *t)).collect(),
    }
}

// Whenever a new type is defined here, add it to ultragraphdb_core_keys' pre-defined types as well.
fn initial_types_internal(namespace: u64, all: bool) -> Vec<pb::TypeUpdate> {
    let mut initial_types = vec![
        type_update(
            "dgraph.graphql",
            &[
                ("dgraph.graphql.schema", ValType::String),
                ("dgraph.graphql.xid", ValType::String),
            ],
        ),
        type_update(
            "dgraph.graphql.persisted_query",
            &[("dgraph.graphql.p_query", ValType::String)],
        ),
    ];

    if namespace == ROOT_NAMESPACE {
        initial_types.push(type_update(
            "dgraph.namespace",
            &[
                ("dgraph.namespace.name", ValType::String),
                ("dgraph.namespace.id", ValType::Int),
            ],
        ));
    }

    if all {
        // These type definitions are required for deleteUser and deleteGroup GraphQL API to work properly.
        initial_types.extend([
            type_update(
                "dgraph.type.User",
                &[
                    ("dgraph.xid", ValType::String),
                    ("dgraph.password", ValType::Password),
                    ("dgraph.user.group", ValType::Uid),
                ],
            ),
            type_update(
                "dgraph.type.Group",
                &[
                    ("dgraph.xid", ValType::String),
                    ("dgraph.acl.rule", ValType::Uid),
                ],
            ),
            type_update(
                "dgraph.type.Rule",
                &[
                    ("dgraph.rule.predicate", ValType::String),
                    ("dgraph.rule.permission", ValType::Int),
                ],
            ),
        ]);
    }

    for typ in &mut initial_types {
        typ.type_name = namespace_attr(namespace, &typ.type_name);
        for field in &mut typ.fields {
            field.predicate = namespace_attr(namespace, &field.predicate);
        }
    }
    initial_types
}

fn initial_schema_internal(namespace: u64, all: bool) -> Vec<pb::SchemaUpdate> {
    let mut dgraph_type = indexed("dgraph.type", ValType::String, "exact");
    dgraph_type.list = true;
    let mut graphql_xid = indexed("dgraph.graphql.xid", ValType::String, "exact");
    graphql_xid.upsert = true;
    let mut initial_schema = vec![
        dgraph_type,
        su("dgraph.drop.op", ValType::String),
        su("dgraph.graphql.schema", ValType::String),
        graphql_xid,
        indexed("dgraph.graphql.p_query", ValType::String, "sha256"),
    ];

    if namespace == ROOT_NAMESPACE {
        let mut name = indexed("dgraph.namespace.name", ValType::String, "exact");
        name.unique = true;
        name.upsert = true;
        let mut id = indexed("dgraph.namespace.id", ValType::Int, "int");
        id.unique = true;
        id.upsert = true;
        initial_schema.extend([name, id]);
    }

    if all {
        // The schema updates of the ACL predicates.
        let mut xid = indexed("dgraph.xid", ValType::String, "exact");
        xid.upsert = true;
        xid.unique = IS_UNIQUE_DGRAPH_XID;
        let mut user_group = su("dgraph.user.group", ValType::Uid);
        user_group.set_directive(Directive::Reverse);
        user_group.list = true;
        let mut acl_rule = su("dgraph.acl.rule", ValType::Uid);
        acl_rule.list = true;
        let mut rule_predicate = indexed("dgraph.rule.predicate", ValType::String, "exact");
        rule_predicate.upsert = true; // Not really sure if this will work.
        initial_schema.extend([
            xid,
            su("dgraph.password", ValType::Password),
            user_group,
            acl_rule,
            rule_predicate,
            su("dgraph.rule.permission", ValType::Int),
        ]);
    }
    for sch in &mut initial_schema {
        sch.predicate = namespace_attr(namespace, &sch.predicate);
    }
    initial_schema
}
