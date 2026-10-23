/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Type declarations: `type Name { field ... }`.

use std::collections::HashSet;

use ultragraphdb_core_keys::{namespace_attr, parse_attr};
use ultragraphdb_core_lex::{ITEM_EOF, Item, ItemIterator};
use ultragraphdb_proto_pb::pb;

use crate::errors::schema_error::SchemaError;
use crate::state::{
    ITEM_COLON, ITEM_EXCLAMATION_MARK, ITEM_LEFT_CURL, ITEM_LEFT_SQUARE, ITEM_NEW_LINE,
    ITEM_RIGHT_CURL, ITEM_RIGHT_SQUARE, ITEM_TEXT,
};

/// Dgraph's `isTypeDeclaration`: whether `item` is the keyword `type` followed by a name and `{`.
pub(crate) fn is_type_declaration(item: &Item<'_>, it: &ItemIterator<'_, '_>) -> bool {
    if item.val() != "type" {
        return false;
    }
    match it.peek(2) {
        Ok(next_items) => next_items[0].typ() == ITEM_TEXT && next_items[1].typ() == ITEM_LEFT_CURL,
        Err(_) => false,
    }
}

/// Dgraph's `parseTypeDeclaration`, from the keyword `type`, which the iterator is on, through the closing `}`.
pub(crate) fn parse_type_declaration(
    it: &mut ItemIterator<'_, '_>,
    ns: u64,
) -> Result<pb::TypeUpdate, SchemaError> {
    if it.item().typ() != ITEM_TEXT || it.item().val() != "type" {
        return Err(it
            .item()
            .errorf(format!("Expected type keyword. Got {}", it.item().val()))
            .into());
    }

    it.next();
    if it.item().typ() != ITEM_TEXT {
        return Err(it
            .item()
            .errorf(format!("Expected type name. Got {}", it.item().val()))
            .into());
    }
    let type_name = namespace_attr(ns, it.item().val());

    it.next();
    if it.item().typ() != ITEM_LEFT_CURL {
        return Err(it
            .item()
            .errorf(format!("Expected {{. Got {}", it.item().val()))
            .into());
    }

    let mut fields: Vec<pb::SchemaUpdate> = Vec::new();
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_RIGHT_CURL => {
                it.next();
                if it.item().typ() != ITEM_NEW_LINE && it.item().typ() != ITEM_EOF {
                    return Err(it
                        .item()
                        .errorf(format!(
                            "Expected new line or EOF after type declaration. Got {}",
                            it.item()
                        ))
                        .into());
                }
                it.prev();

                let mut field_set = HashSet::new();
                for field in &fields {
                    if !field_set.insert(field.predicate.as_str()) {
                        return Err(it
                            .item()
                            .errorf(format!(
                                "Duplicate fields with name: {}",
                                parse_attr(&field.predicate)?
                            ))
                            .into());
                    }
                }
                return Ok(pb::TypeUpdate { type_name, fields });
            }
            ITEM_TEXT => fields.push(parse_type_field(it, ns)?),
            ITEM_NEW_LINE => {
                // Ignore empty lines.
            }
            _ => {
                return Err(it
                    .item()
                    .errorf(format!("Unexpected token. Got {}", it.item().val()))
                    .into());
            }
        }
    }
    Err(SchemaError::parse("Shouldn't reach here."))
}

/// Dgraph's `parseTypeField`: a field's name, which the iterator is on, through the end of its line. The old
/// format, `name: [type!]!`, is accepted for backwards compatibility and its type ignored; Dgraph logs a warning
/// about it.
fn parse_type_field(
    it: &mut ItemIterator<'_, '_>,
    ns: u64,
) -> Result<pb::SchemaUpdate, SchemaError> {
    let field = pb::SchemaUpdate {
        predicate: namespace_attr(ns, it.item().val()),
        ..Default::default()
    };
    it.next();

    // Simplified type definitions only require the field name. If a new line is found, proceed to the next field
    // in the type.
    if it.item().typ() == ITEM_NEW_LINE {
        return Ok(field);
    }

    // For the sake of backwards-compatibility, process type definitions in the old format, but ignore the
    // information after the colon.
    if it.item().typ() != ITEM_COLON {
        return Err(it
            .item()
            .errorf(format!(
                "Missing colon in type declaration. Got {}",
                it.item().val()
            ))
            .into());
    }

    it.next();
    let list = it.item().typ() == ITEM_LEFT_SQUARE;
    if list {
        it.next();
    }

    if it.item().typ() != ITEM_TEXT {
        return Err(it
            .item()
            .errorf(format!(
                "Missing field type in type declaration. Got {}",
                it.item().val()
            ))
            .into());
    }

    it.next();
    if it.item().typ() == ITEM_EXCLAMATION_MARK {
        it.next();
    }

    if list {
        if it.item().typ() != ITEM_RIGHT_SQUARE {
            return Err(it
                .item()
                .errorf(format!(
                    "Expected matching square bracket. Got {}",
                    it.item().val()
                ))
                .into());
        }
        it.next();

        if it.item().typ() == ITEM_EXCLAMATION_MARK {
            it.next();
        }
    }

    if it.item().typ() != ITEM_NEW_LINE {
        return Err(it
            .item()
            .errorf(format!(
                "Expected new line after field declaration. Got {}",
                it.item().val()
            ))
            .into());
    }
    Ok(field)
}
