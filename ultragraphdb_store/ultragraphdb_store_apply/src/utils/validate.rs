/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/mutation.go (isStarAll, isDeletePredicateEdge, ValidateAndConvert,
 * typeSanityCheck) and posting/list.go (TypeID). Changed for ultragraphdb: translated to Rust; the ACL check of
 * `dgraph.rule.permission` is left to the stage that brings ACLs.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{STAR, parse_attr};
use ultragraphdb_core_tok::VEC_KEYWORD;
use ultragraphdb_core_types::{TypeId, Val, Value, convert, marshal};
use ultragraphdb_proto_pb::pb::{DirectedEdge, SchemaUpdate, TypeUpdate, posting, schema_update};

use crate::errors::apply_error::ApplyError;
use crate::utils::edge_text::edge_text;

/// Whether `v` is the star of a delete-all.
pub(crate) fn is_star_all(v: &[u8]) -> bool {
    v == STAR.as_bytes()
}

/// Whether `edge` drops its predicate: `* <p> *`.
pub(crate) fn is_delete_predicate_edge(edge: &DirectedEdge) -> bool {
    edge.entity == 0 && is_star_all(&edge.value)
}

/// Dgraph's `posting.TypeID`: the type an edge stores, uid for a reference.
pub(crate) fn type_of_edge(edge: &DirectedEdge) -> TypeId {
    if edge.value_id != 0 {
        TypeId::UID
    } else {
        TypeId::new(edge.value_type)
    }
}

/// The predicate of a namespaced attribute, as Dgraph's `x.ParseAttr` prints it.
pub(crate) fn attr_name(attr: &str) -> String {
    parse_attr(attr).unwrap_or(attr).to_string()
}

/// Dgraph's `ValidateAndConvert`: checks `edge` against the predicate's schema `su` and converts its value to the
/// schema's type, or, without one, to the type the edge names. A predicate drop and a delete-all pass unchanged.
///
/// # Errors
///
/// A language for a predicate without `@lang`; a write to the predicates a vector index keeps; a scalar for a uid
/// predicate and the reverse; a value for a vector predicate that is no vector; and the conversion's error.
pub fn validate_and_convert(edge: &mut DirectedEdge, su: &SchemaUpdate) -> Result<(), ApplyError> {
    if is_delete_predicate_edge(edge) {
        return Ok(());
    }
    if TypeId::new(edge.value_type) == TypeId::DEFAULT && is_star_all(&edge.value) {
        return Ok(());
    }
    if su.predicate.contains(VEC_KEYWORD) {
        return Err(ApplyError::VectorKeyword(edge_text(edge)));
    }
    let storage_type = type_of_edge(edge);
    let mut schema_type = TypeId::new(su.value_type);
    if !edge.lang.is_empty() && !su.lang {
        return Err(ApplyError::LangDirective(
            attr_name(&edge.attr),
            edge_text(edge),
        ));
    }
    match (schema_type.is_scalar(), storage_type.is_scalar()) {
        (false, false) => return Ok(()),
        (false, true) => {
            return Err(ApplyError::UidIsScalar(
                attr_name(&edge.attr),
                edge_text(edge),
            ));
        }
        (true, false) => {
            return Err(ApplyError::ScalarIsUid(
                attr_name(&edge.attr),
                edge_text(edge),
            ));
        }
        (true, true) => {}
    }
    if schema_type == TypeId::VFLOAT {
        if ![TypeId::VFLOAT, TypeId::STRING, TypeId::DEFAULT].contains(&storage_type) {
            return Err(ApplyError::NotVector(
                attr_name(&edge.attr),
                edge_text(edge),
            ));
        }
    } else if storage_type == schema_type && schema_type != TypeId::DEFAULT {
        return Ok(());
    } else if schema_type == TypeId::DEFAULT {
        schema_type = storage_type;
    }

    let src = Val::new(
        TypeId::new(edge.value_type),
        Value::Bytes(edge.value.clone()),
    );
    let dst = convert(&src, schema_type)?;
    let b = marshal(&dst, TypeId::BINARY)?;
    edge.value_type = schema_type.enum_value();
    match b.into_value() {
        Some(Value::Bytes(bytes)) => {
            edge.value = bytes;
            Ok(())
        }
        _ => Err(ApplyError::ConvertEdgeType(
            storage_type.enum_value().to_string(),
            schema_type.enum_value().to_string(),
        )),
    }
}

/// Dgraph's `typeSanityCheck`: every field of a type has a name, an object field its object type's name, and no
/// field a directive or tokenizers.
pub fn type_sanity_check(t: &TypeUpdate) -> Result<(), ApplyError> {
    for field in &t.fields {
        if attr_name(&field.predicate).is_empty() {
            return Err(ApplyError::FieldName());
        }
        if field.value_type == posting::ValType::Object as i32 && field.object_type_name.is_empty()
        {
            return Err(ApplyError::FieldObjectName());
        }
        if field.directive != schema_update::Directive::None as i32 {
            return Err(ApplyError::FieldDirective());
        }
        if !field.tokenizer.is_empty() {
            return Err(ApplyError::FieldTokenizers());
        }
    }
    Ok(())
}
