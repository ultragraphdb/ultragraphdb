/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The schema parser: predicates with their value types and directives, type declarations and namespaces.

mod parse_index;
mod parse_namespace;
mod parse_type;
mod resolve_tokenizers;

use ultragraphdb_common_utils::go_strings;
use ultragraphdb_core_keys::{ROOT_NAMESPACE, namespace_attr};
use ultragraphdb_core_lex::{ITEM_EOF, Item, ItemIterator, Lexer};
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

use crate::errors::schema_error::SchemaError;
use crate::parse::parse_index::parse_index_directive;
use crate::parse::parse_namespace::parse_namespace;
use crate::parse::parse_type::{is_type_declaration, parse_type_declaration};
use crate::parse::resolve_tokenizers::resolve_tokenizers;
use crate::state::{
    ITEM_AT, ITEM_COLON, ITEM_DOT, ITEM_LEFT_SQUARE, ITEM_NEW_LINE, ITEM_RIGHT_SQUARE, ITEM_TEXT,
    LEX_TEXT,
};
use crate::types::parsed_schema::ParsedSchema;

/// The namespace that tells [`parse_namespaced`] to keep each predicate's own namespace, Go's `math.MaxUint64`.
const PRESERVE_NAMESPACE: u64 = u64::MAX;

/// Dgraph's `Parse`: parses schema text, keeping the namespace a `[0x..]` prefix names and placing everything
/// without one in the root namespace.
///
/// # Errors
///
/// A [`SchemaError`] with Dgraph's text when the schema is invalid.
pub fn parse(s: &str) -> Result<ParsedSchema, SchemaError> {
    parse_namespaced(s, PRESERVE_NAMESPACE)
}

/// Dgraph's `ParseWithNamespace`: parses schema text and places every predicate and type in `namespace`, whatever
/// namespace its text names. `u64::MAX` keeps them, as [`parse`] does.
///
/// # Errors
///
/// A [`SchemaError`] with Dgraph's text when the schema is invalid.
pub fn parse_with_namespace(s: &str, namespace: u64) -> Result<ParsedSchema, SchemaError> {
    parse_namespaced(s, namespace)
}

/// Dgraph's `HasTokenizerOrVectorIndexSpec`: whether the update has a tokenizer or a vector index.
pub fn has_tokenizer_or_vector_index_spec(update: &pb::SchemaUpdate) -> bool {
    !update.tokenizer.is_empty() || !update.index_specs.is_empty()
}

/// Dgraph's `parse`: with `namespace == u64::MAX` each predicate keeps its namespace, otherwise all go to
/// `namespace`.
fn parse_namespaced(s: &str, namespace: u64) -> Result<ParsedSchema, SchemaError> {
    let mut l = Lexer::new(s);
    l.run(LEX_TEXT);
    l.validate_result()?;

    let mut preds = Vec::new();
    let mut types = Vec::new();
    let mut it = l.new_iterator();
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_EOF => {
                resolve_tokenizers(&preds).map_err(|e| e.wrap("failed to enrich schema"))?;
                return Ok(ParsedSchema::new(preds, types));
            }
            ITEM_TEXT => {
                // For schema which does not contain the namespace information, use the default namespace, if
                // namespace has to be preserved. Else, use the passed namespace.
                let ns = if namespace == PRESERVE_NAMESPACE {
                    ROOT_NAMESPACE
                } else {
                    namespace
                };
                parse_type_or_schema(item, &mut it, ns, &mut preds, &mut types)?;
            }
            ITEM_LEFT_SQUARE => {
                // We expect a namespace.
                let mut ns =
                    parse_namespace(&mut it).map_err(|e| e.wrap("While parsing namespace:"))?;
                if namespace != PRESERVE_NAMESPACE {
                    // Use the passed namespace, if we don't want to preserve the namespace.
                    ns = namespace;
                }
                // parse_namespace has already moved onto the item after the namespace.
                let item = it.item();
                parse_type_or_schema(item, &mut it, ns, &mut preds, &mut types)?;
            }
            ITEM_NEW_LINE => {
                // pass empty line
            }
            _ => {
                return Err(it
                    .item()
                    .errorf(format!("Unexpected token: {item} while parsing schema"))
                    .into());
            }
        }
    }
    Err(SchemaError::parse("Shouldn't reach here"))
}

/// A type declaration or a predicate, at the item that starts it.
fn parse_type_or_schema(
    item: &Item<'_>,
    it: &mut ItemIterator<'_, '_>,
    ns: u64,
    preds: &mut Vec<pb::SchemaUpdate>,
    types: &mut Vec<pb::TypeUpdate>,
) -> Result<(), SchemaError> {
    if is_type_declaration(item, it) {
        types.push(parse_type_declaration(it, ns)?);
    } else {
        preds.push(parse_scalar_pair(it, item.val(), ns)?);
    }
    Ok(())
}

/// Dgraph's `parseScalarPair`: a predicate's value type and directives, through the terminating `.`.
fn parse_scalar_pair(
    it: &mut ItemIterator<'_, '_>,
    predicate: &str,
    ns: u64,
) -> Result<pb::SchemaUpdate, SchemaError> {
    it.next();
    let mut next = it.item();
    // Two forms are disallowed: `name@en: string .`, which the `@` item catches, and `<name@en>: string .`, which
    // the check of the name catches. '@' is not allowed in predicate names.
    if next.typ() == ITEM_AT || predicate.contains('@') {
        return Err(next.errorf("Invalid '@' in name").into());
    }
    if next.typ() != ITEM_COLON {
        return Err(next.errorf("Missing colon").into());
    }
    if !it.next() {
        return Err(next
            .errorf("Invalid ending while trying to parse schema.")
            .into());
    }
    next = it.item();
    let mut schema = pb::SchemaUpdate {
        predicate: namespace_attr(ns, predicate),
        ..Default::default()
    };
    // Could be list type.
    if next.typ() == ITEM_LEFT_SQUARE {
        schema.list = true;
        if !it.next() {
            return Err(next
                .errorf("Invalid ending while trying to parse schema.")
                .into());
        }
        next = it.item();
    }

    if next.typ() != ITEM_TEXT {
        return Err(next.errorf("Missing Type").into());
    }
    // We ignore the case for types.
    let Some(t) = TypeId::for_name(&go_strings::to_lower(next.val())) else {
        return Err(next.errorf("Undefined Type").into());
    };
    if schema.list && (t == TypeId::PASSWORD || t == TypeId::BOOL) {
        return Err(next
            .errorf(format!("Unsupported type for list: [{}].", t.name()))
            .into());
    }
    schema.value_type = t.enum_value();

    // Check for index / reverse.
    it.next();
    next = it.item();
    if schema.list {
        if next.typ() != ITEM_RIGHT_SQUARE {
            return Err(next
                .errorf(format!("Unclosed [ while parsing schema for: {predicate}"))
                .into());
        }
        if !it.next() {
            return Err(next.errorf("Invalid ending").into());
        }
        next = it.item();
    }

    while next.typ() == ITEM_AT {
        parse_directive(it, &mut schema, t)?;
        next = it.item();
    }

    if next.typ() != ITEM_DOT {
        return Err(next.errorf("Invalid ending").into());
    }
    it.next();
    next = it.item();
    if next.typ() == ITEM_EOF {
        it.prev();
        return Ok(schema);
    }
    if next.typ() != ITEM_NEW_LINE {
        return Err(next.errorf("Invalid ending").into());
    }
    Ok(schema)
}

/// Dgraph's `parseDirective`: the directive after an `@`, leaving the iterator on the item after it.
fn parse_directive(
    it: &mut ItemIterator<'_, '_>,
    schema: &mut pb::SchemaUpdate,
    t: TypeId,
) -> Result<(), SchemaError> {
    it.next();
    let next = it.item();
    if next.typ() != ITEM_TEXT {
        return Err(next.errorf("Missing directive name").into());
    }
    match next.val() {
        "reverse" => {
            if t != TypeId::UID {
                return Err(next.errorf("Cannot reverse for non-UID type").into());
            }
            schema.set_directive(Directive::Reverse);
        }
        "index" => {
            let (tokenizer, vector_specs) = parse_index_directive(it, &schema.predicate, t)?;
            schema.set_directive(Directive::Index);
            schema.tokenizer = tokenizer;
            schema.index_specs = vector_specs;
        }
        "count" => schema.count = true,
        "upsert" => schema.upsert = true,
        "unique" => schema.unique = true,
        "noconflict" => schema.no_conflict = true,
        "lang" => {
            if t != TypeId::STRING || schema.list {
                return Err(next
                    .errorf(format!(
                        "@lang directive can only be specified for string type. Got: [{}] for attr: [{}]",
                        t.name(),
                        schema.predicate
                    ))
                    .into());
            }
            schema.lang = true;
        }
        _ => return Err(next.errorf("Invalid index specification").into()),
    }
    it.next();
    Ok(())
}
