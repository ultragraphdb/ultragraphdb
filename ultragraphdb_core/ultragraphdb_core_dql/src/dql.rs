/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/dql.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_lex::Lexer;
use ultragraphdb_proto_dgraph::api;

use crate::errors::dql_error::DqlError;
use crate::parser::iterr;
use crate::parser_mutation::{parse_mutation_block, parse_upsert_block};
use crate::state::{ITEM_MUTATION_OP, ITEM_UPSERT_BLOCK, LEX_TOP_LEVEL};

/// Sorts a DQL request into its kind: an upsert block or a mutation block becomes its mutations, anything else
/// is a query, kept as text.
///
/// # Errors
///
/// Dgraph's lexer error text, or the parser's for an upsert or mutation block.
pub fn parse_dql(dql_query: &str) -> Result<api::Request, DqlError> {
    let mut lexer = Lexer::new(dql_query);
    lexer.run(LEX_TOP_LEVEL);
    lexer.validate_result()?;

    let mut it = lexer.new_iterator();
    if !it.next() {
        return Err(iterr(&it, "Invalid mutation"));
    }
    if it.item().typ() == ITEM_UPSERT_BLOCK {
        it.prev();
        return parse_upsert_block(&mut it);
    }
    it.next();
    if it.item().typ() == ITEM_MUTATION_OP {
        it.prev();
        it.prev();
        let mu = parse_mutation_block(&mut it)?;
        return Ok(api::Request {
            mutations: vec![mu],
            ..api::Request::default()
        });
    }
    Ok(api::Request {
        query: dql_query.to_string(),
        ..api::Request::default()
    })
}
