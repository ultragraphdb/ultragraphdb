/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser_mutation.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Mutation blocks and upsert blocks, their N-Quads and conditions kept as text.

use ultragraphdb_core_lex::{ITEM_EOF, Lexer};
use ultragraphdb_proto_dgraph::api;

use crate::errors::dql_error::DqlError;
use crate::parser::{It, iterr};
use crate::state::{
    ITEM_LEFT_CURL, ITEM_MUTATION_OP, ITEM_MUTATION_OP_CONTENT, ITEM_RIGHT_CURL, ITEM_TEXT,
    ITEM_UPSERT_BLOCK, ITEM_UPSERT_BLOCK_OP, ITEM_UPSERT_BLOCK_OP_CONTENT, LEX_TOP_LEVEL,
};

/// Parses a mutation block, or an upsert block with its query and mutations, into a request.
///
/// # Errors
///
/// Dgraph's lexer or parser error text; a request must be one block.
pub fn parse_mutation(mutation: &str) -> Result<api::Request, DqlError> {
    let mut lexer = Lexer::new(mutation);
    lexer.run(LEX_TOP_LEVEL);
    lexer.validate_result()?;

    let mut it = lexer.new_iterator();
    if !it.next() {
        return Err(iterr(&it, "Invalid mutation"));
    }
    let item = it.item();
    let req = match item.typ() {
        ITEM_UPSERT_BLOCK => parse_upsert_block(&mut it)?,
        ITEM_LEFT_CURL => api::Request {
            mutations: vec![parse_mutation_block(&mut it)?],
            ..api::Request::default()
        },
        _ => return Err(iterr(&it, format!("Unexpected token: [{}]", item.val()))),
    };
    // mutations must be enclosed in a single block.
    if it.next() && it.item().typ() != ITEM_EOF {
        return Err(iterr(
            &it,
            format!("Unexpected {} after the end of the block", it.item().val()),
        ));
    }
    Ok(req)
}

/// Parses the upsert block whose keyword is the current item.
pub(crate) fn parse_upsert_block(it: &mut It<'_, '_>) -> Result<api::Request, DqlError> {
    let mut req: Option<api::Request> = None;
    let mut query_text = String::new();
    let mut cond_text = String::new();
    let mut query_found = false;

    // ===>upsert<=== {...}
    if !it.next() {
        return Err(iterr(it, "Unexpected end of upsert block"));
    }
    // upsert ===>{<=== ....}
    let item = it.item();
    if item.typ() != ITEM_LEFT_CURL {
        return Err(iterr(
            it,
            format!("Expected {{ at the start of block. Got: [{}]", item.val()),
        ));
    }

    while it.next() {
        let item = it.item();
        if item.typ() == ITEM_RIGHT_CURL {
            // upsert {... ===>}<===
            let Some(mut req) = req else {
                return Err(iterr(it, "Empty mutation block"));
            };
            if !query_found {
                return Err(iterr(it, "Query op not found in upsert block"));
            }
            req.query = query_text;
            return Ok(req);
        } else if item.typ() == ITEM_UPSERT_BLOCK_OP && item.val() == "query" {
            // upsert { mutation{...} ===>query<==={...}}
            if query_found {
                return Err(iterr(it, "Multiple query ops inside upsert block"));
            }
            query_found = true;
            if !it.next() {
                return Err(iterr(it, "Unexpected end of upsert block"));
            }
            let item = it.item();
            if item.typ() != ITEM_UPSERT_BLOCK_OP_CONTENT {
                return Err(iterr(
                    it,
                    format!("Expecting brace, found '{}'", item.val()),
                ));
            }
            query_text.push_str(item.val());
        } else if item.typ() == ITEM_UPSERT_BLOCK_OP && item.val() == "mutation" {
            // upsert { ===>mutation<=== {...} query{...}}
            if !it.next() {
                return Err(iterr(it, "Unexpected end of upsert block"));
            }
            // upsert { mutation ===>@if(...)<=== {....} query{...}}
            if it.item().typ() == ITEM_UPSERT_BLOCK_OP_CONTENT {
                cond_text = it.item().val().to_string();
                if !it.next() {
                    return Err(iterr(it, "Unexpected end of upsert block"));
                }
            }
            // upsert @if(...) ===>{<=== ....}
            let mut mu = parse_mutation_block(it)?;
            mu.cond = cond_text.clone();
            req.get_or_insert_with(api::Request::default)
                .mutations
                .push(mu);
        } else if item.typ() == ITEM_UPSERT_BLOCK_OP && item.val() == "fragment" {
            // upsert { mutation{...} ===>fragment<==={...}}
            if !it.next() {
                return Err(iterr(it, "Unexpected end of upsert block"));
            }
            let item = it.item();
            if item.typ() != ITEM_UPSERT_BLOCK_OP_CONTENT {
                return Err(iterr(
                    it,
                    format!("Expecting brace, found '{}'", item.val()),
                ));
            }
            query_text.push_str("fragment");
            query_text.push_str(item.val());
        } else {
            return Err(iterr(
                it,
                format!("Unexpected token in upsert block [{}]", item.val()),
            ));
        }
    }
    Err(iterr(it, "Invalid upsert block"))
}

/// Parses the mutation block whose `{` is the current item.
pub(crate) fn parse_mutation_block(it: &mut It<'_, '_>) -> Result<api::Mutation, DqlError> {
    let mut mu = api::Mutation::default();
    let item = it.item();
    if item.typ() != ITEM_LEFT_CURL {
        return Err(iterr(
            it,
            format!("Expected {{ at the start of block. Got: [{}]", item.val()),
        ));
    }
    while it.next() {
        let item = it.item();
        if item.typ() == ITEM_TEXT {
            continue;
        }
        if item.typ() == ITEM_RIGHT_CURL {
            return Ok(mu);
        }
        if item.typ() == ITEM_MUTATION_OP {
            parse_mutation_op(it, item.val(), &mut mu)?;
        }
    }
    Err(iterr(it, "Invalid mutation."))
}

/// Parses a set or delete operation and stores its text in the mutation.
fn parse_mutation_op(
    it: &mut It<'_, '_>,
    op: &str,
    mu: &mut api::Mutation,
) -> Result<(), DqlError> {
    let mut parse = false;
    while it.next() {
        let item = it.item();
        if item.typ() == ITEM_TEXT {
            continue;
        }
        if item.typ() == ITEM_LEFT_CURL {
            if parse {
                return Err(iterr(it, "Too many left curls in set mutation."));
            }
            parse = true;
        }
        if item.typ() == ITEM_MUTATION_OP_CONTENT {
            if !parse {
                return Err(iterr(it, "Mutation syntax invalid."));
            }
            match op {
                "set" => mu.set_nquads = item.val().as_bytes().to_vec(),
                "delete" => mu.del_nquads = item.val().as_bytes().to_vec(),
                "schema" => {
                    return Err(iterr(
                        it,
                        "Altering schema not supported through http client.",
                    ));
                }
                "dropall" => return Err(iterr(it, "Dropall not supported through http client.")),
                _ => return Err(iterr(it, "Invalid mutation operation.")),
            }
        }
        if item.typ() == ITEM_RIGHT_CURL {
            return Ok(());
        }
    }
    Err(iterr(it, "Invalid mutation formatting."))
}
