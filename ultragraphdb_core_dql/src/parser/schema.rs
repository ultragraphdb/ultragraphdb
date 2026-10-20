/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The schema block, `schema(pred: [name, age]) { type index }`.

use ultragraphdb_proto_pb::pb;

use super::{It, collect_name, ierr, iterr};
use crate::errors::dql_error::DqlError;
use crate::state::{
    ITEM_COLON, ITEM_COMMA, ITEM_LEFT_CURL, ITEM_LEFT_ROUND, ITEM_LEFT_SQUARE, ITEM_NAME,
    ITEM_RIGHT_CURL, ITEM_RIGHT_ROUND, ITEM_RIGHT_SQUARE,
};

/// Parses `a, b]` of a list whose `[` was read.
fn parse_list_item_names(it: &mut It<'_, '_>) -> Result<Vec<String>, DqlError> {
    let mut items = Vec::new();
    while it.next() {
        let mut item = it.item();
        match item.typ() {
            ITEM_RIGHT_SQUARE => return Ok(items),
            ITEM_NAME => items.push(collect_name(it, item.val())),
            ITEM_COMMA => {
                it.next();
                item = it.item();
                if item.typ() != ITEM_NAME {
                    return Err(ierr(item, "Invalid scheam block"));
                }
                items.push(collect_name(it, item.val()));
            }
            _ => return Err(ierr(item, "Invalid schema block")),
        }
    }
    Err(iterr(it, "Expecting ] to end list but none was found"))
}

/// Parses `pred: a)` or `type: [a, b])`.
fn parse_schema_preds_or_types(
    it: &mut It<'_, '_>,
    s: &mut pb::SchemaRequest,
) -> Result<(), DqlError> {
    // pred or type should be followed by colon
    it.next();
    let mut item = it.item();
    if item.typ() != ITEM_NAME && !(item.val() == "pred" || item.val() == "type") {
        return Err(ierr(item, "Invalid schema block"));
    }
    let parse_types = item.val() == "type";

    it.next();
    item = it.item();
    if item.typ() != ITEM_COLON {
        return Err(ierr(item, "Invalid schema block"));
    }

    // can be a or [a,b]
    it.next();
    item = it.item();
    match item.typ() {
        ITEM_NAME => {
            if parse_types {
                s.types.push(item.val().to_string());
            } else {
                s.predicates.push(item.val().to_string());
            }
        }
        ITEM_LEFT_SQUARE => {
            let names = parse_list_item_names(it)?;
            if parse_types {
                s.types = names;
            } else {
                s.predicates = names;
            }
        }
        _ => return Err(ierr(item, "Invalid schema block")),
    }

    it.next();
    item = it.item();
    if item.typ() == ITEM_RIGHT_ROUND {
        return Ok(());
    }
    Err(ierr(item, "Invalid schema blocks"))
}

/// Parses the fields up to the closing `}`.
fn parse_schema_fields(it: &mut It<'_, '_>, s: &mut pb::SchemaRequest) -> Result<(), DqlError> {
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_RIGHT_CURL => return Ok(()),
            ITEM_NAME => s.fields.push(item.val().to_string()),
            _ => return Err(ierr(item, "Invalid schema block.")),
        }
    }
    Err(iterr(
        it,
        "Expecting } to end fields list, but none was found",
    ))
}

pub(crate) fn get_schema(it: &mut It<'_, '_>) -> Result<pb::SchemaRequest, DqlError> {
    let mut s = pb::SchemaRequest::default();
    let mut left_round_seen = false;
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_LEFT_CURL => {
                parse_schema_fields(it, &mut s)?;
                return Ok(s);
            }
            ITEM_LEFT_ROUND => {
                if left_round_seen {
                    return Err(ierr(item, "Too many left rounds in schema block"));
                }
                left_round_seen = true;
                parse_schema_preds_or_types(it, &mut s)?;
            }
            _ => return Err(ierr(item, "Invalid schema block")),
        }
    }
    Err(iterr(it, "Invalid schema block."))
}
