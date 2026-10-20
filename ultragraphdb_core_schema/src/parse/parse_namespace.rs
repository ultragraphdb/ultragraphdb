/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_lex::ItemIterator;

use crate::errors::schema_error::SchemaError;
use crate::state::{ITEM_NUMBER, ITEM_RIGHT_SQUARE};
use crate::utils::go_strconv::parse_uint_base0;

/// Dgraph's `parseNamespace`: the namespace of a `[0x..]` prefix, whose `[` the iterator is on, leaving the
/// iterator on the item after the `]`. A decimal, octal (`0` first) or hexadecimal (`0x`) number.
pub(crate) fn parse_namespace(it: &mut ItemIterator<'_, '_>) -> Result<u64, SchemaError> {
    let next_items = it
        .peek(2)
        .map_err(|err| SchemaError::parse(format!("Unable to peek: {err}")))?;
    if next_items[0].typ() != ITEM_NUMBER || next_items[1].typ() != ITEM_RIGHT_SQUARE {
        return Err(SchemaError::parse("Typed oes not match the expected"));
    }
    let ns = parse_uint_base0(next_items[0].val()).map_err(SchemaError::parse)?;
    it.next();
    it.next();
    // We have parsed the namespace. Now move to the next item.
    if !it.next() {
        return Err(SchemaError::parse(format!(
            "No schema found after namespace. Got: {}",
            next_items[0]
        )));
    }
    Ok(ns)
}
