/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The `@index` directive: its tokenizers and its vector index specifications.

use std::collections::HashSet;

use ultragraphdb_common_utils::go_strings;
use ultragraphdb_core_keys::parse_attr;
use ultragraphdb_core_lex::ItemIterator;
use ultragraphdb_core_tok::{get_index_factory, get_tokenizer};
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb;

use crate::errors::schema_error::SchemaError;
use crate::state::{
    ITEM_COLON, ITEM_COMMA, ITEM_LEFT_ROUND, ITEM_QUOTED_TEXT, ITEM_RIGHT_ROUND, ITEM_TEXT,
};

/// A token of the `@index` list: a tokenizer's name, or a vector index specification.
enum Token {
    Tokenizer(String),
    VectorIndex(pb::VectorIndexSpec),
}

/// Dgraph's `parseIndexDirective`: the list after `@index`, which the iterator is on.
///
/// ```text
/// Tokens    ::= '(' TokenList ')'
/// TokenList ::= Token [',' TokenList]
/// ```
pub(crate) fn parse_index_directive(
    it: &mut ItemIterator<'_, '_>,
    predicate: &str,
    typ: TypeId,
) -> Result<(Vec<String>, Vec<pb::VectorIndexSpec>), SchemaError> {
    let mut tokenizers = Vec::new();
    let mut vector_specs = Vec::new();
    let mut seen = HashSet::new();
    let mut seen_sortable_tok = false;

    if typ == TypeId::UID || typ == TypeId::DEFAULT || typ == TypeId::PASSWORD {
        return Err(it
            .item()
            .errorf(format!(
                "Indexing not allowed on predicate {predicate} of type {}",
                typ.name()
            ))
            .into());
    }
    if !it.next() {
        // Nothing to read.
        return Err(it.item().errorf("Invalid ending.").into());
    }
    let mut next = it.item();
    if next.typ() != ITEM_LEFT_ROUND {
        it.prev(); // Backup.
        return Err(it
            .item()
            .errorf(format!(
                "Require type of tokenizer for pred: {predicate} for indexing."
            ))
            .into());
    }

    // Look for tokenizers and index factories (vector specs).
    loop {
        let (token, sortable) = parse_token_or_vector_index_spec(it, predicate, typ)?;
        if sortable && seen_sortable_tok {
            return Err(next
                .errorf(format!(
                    "Only one index tokenizer can be sortable for {predicate}"
                ))
                .into());
        }
        // As in Dgraph, the flag follows the last token rather than accumulating.
        seen_sortable_tok = sortable;
        match token {
            Token::Tokenizer(name) => {
                if seen.contains(&name) {
                    return Err(next
                        .errorf(format!(
                            "Duplicate tokenizers defined for predicate {predicate}"
                        ))
                        .into());
                }
                seen.insert(name.clone());
                tokenizers.push(name);
            }
            Token::VectorIndex(spec) => {
                // Two vector index specifications of the same name are accepted: "seen" holds the name, but only
                // tokenizers are checked against it.
                seen.insert(spec.name.clone());
                vector_specs.push(spec);
            }
        }

        it.next();
        next = it.item();
        if next.typ() == ITEM_RIGHT_ROUND {
            break;
        }
        if next.typ() != ITEM_COMMA {
            return Err(next
                .errorf(format!(
                    "Expected ',' or ')' but found '{}' for predicate '{predicate}'",
                    next.val()
                ))
                .into());
        }
    }
    Ok((tokenizers, vector_specs))
}

/// Dgraph's `parseTokenOrVectorIndexSpec`: the name of a tokenizer, or of an index factory with its options, and
/// whether the index is sortable.
///
/// ```text
/// Token     ::= TokenName [ TokenOptions ]
/// TokenName ::= {itemText from Lexer}
/// ```
fn parse_token_or_vector_index_spec(
    it: &mut ItemIterator<'_, '_>,
    predicate: &str,
    typ: TypeId,
) -> Result<(Token, bool), SchemaError> {
    it.next();
    let next = it.item();
    if next.typ() != ITEM_TEXT {
        return Err(next
            .errorf(format!(
                "Expected token or VectorFactory name, but found '{}'",
                next.val()
            ))
            .into());
    }
    let name = go_strings::to_lower(next.val());
    if let Some(factory) = get_index_factory(&name) {
        if typ != TypeId::VFLOAT {
            return Err(next
                .errorf(format!(
                    "IndexFactory: {} isn't valid for predicate: {} of type: {}",
                    factory.name(),
                    parse_attr(predicate),
                    typ.name()
                ))
                .into());
        }
        let options = parse_token_options(it)?;
        let allowed = factory.allowed_options();
        for pair in &options {
            if let Err(err) = allowed.get_parsed_option(&pair.key, &pair.value) {
                return Err(next
                    .errorf(format!(
                        "IndexFactory: {} issues this error: '{err}'",
                        factory.name()
                    ))
                    .into());
            }
        }
        let spec = pb::VectorIndexSpec { name, options };
        // An index factory's IsSortable is false.
        return Ok((Token::VectorIndex(spec), false));
    }

    // Look for a custom tokenizer, and validate its type.
    let Some(tokenizer) = get_tokenizer(&name) else {
        return Err(next
            .errorf(format!("Invalid tokenizer {}", next.val()))
            .into());
    };
    if tokenizer.type_id() != typ {
        return Err(next
            .errorf(format!(
                "Tokenizer: {} isn't valid for predicate: {} of type: {}",
                tokenizer.name(),
                parse_attr(predicate),
                typ.name()
            ))
            .into());
    }
    Ok((Token::Tokenizer(name), tokenizer.is_sortable()))
}

/// Dgraph's `parseTokenOptions`: the optional options of an index factory. No options is valid as long as the
/// next item is a comma (another tokenizer follows) or a closing bracket (the list ends).
///
/// ```text
/// TokenOptions    ::= ['(' TokenOptionList ')']
/// TokenOptionList ::= TokenOption [',' TokenOptionList ]
/// ```
fn parse_token_options(it: &mut ItemIterator<'_, '_>) -> Result<Vec<pb::OptionPair>, SchemaError> {
    let Some(next_item) = it.peek_one() else {
        return Err(it
            .errorf("unexpected end of stream when looking for IndexFactory options")
            .into());
    };
    if next_item.typ() == ITEM_COMMA || next_item.typ() == ITEM_RIGHT_ROUND {
        return Ok(Vec::new());
    }
    if next_item.typ() != ITEM_LEFT_ROUND {
        return Err(next_item
            .errorf(format!(
                "unexpected '{}' found when expecting '('",
                next_item.val()
            ))
            .into());
    }
    it.next(); // Reads the initial '('.
    let mut options = Vec::new();
    loop {
        options.push(parse_token_option(it)?);
        it.next();
        let next_item = it.item();
        if next_item.typ() == ITEM_RIGHT_ROUND {
            return Ok(options);
        }
        if next_item.typ() != ITEM_COMMA {
            return Err(next_item
                .errorf(format!(
                    "unexpected '{}' found when expecting ',' or ')'",
                    next_item.val()
                ))
                .into());
        }
    }
}

/// Dgraph's `parseTokenOption`: one option of an index factory; the factory checks it afterwards.
///
/// ```text
/// TokenOption ::= OptionName ':' OptionValue
/// OptionName  ::= {itemText from Lexer}
/// OptionValue ::= {itemQuotedText from Lexer}
/// ```
fn parse_token_option(it: &mut ItemIterator<'_, '_>) -> Result<pb::OptionPair, SchemaError> {
    it.next();
    let next_item = it.item();
    if next_item.typ() != ITEM_TEXT {
        return Err(next_item
            .errorf(format!(
                "unexpected '{}' found when expecting option name",
                next_item.val()
            ))
            .into());
    }
    let key = next_item.val().to_string();
    it.next();
    let next_item = it.item();
    if next_item.typ() != ITEM_COLON {
        return Err(next_item
            .errorf(format!(
                "unexpected '{}' found when expecting ':'",
                next_item.val()
            ))
            .into());
    }
    it.next();
    let next_item = it.item();
    if next_item.typ() != ITEM_QUOTED_TEXT {
        return Err(next_item
            .errorf(format!(
                "unexpected '{}' found when expecting quoted text",
                next_item.val()
            ))
            .into());
    }
    let quoted = next_item.val();
    let value = quoted[1..quoted.len() - 1].to_string();
    Ok(pb::OptionPair { key, value })
}
