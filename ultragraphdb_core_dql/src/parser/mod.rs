/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The query parser: `Parse` and `ParseWithNeedVars`, and the helpers every part of the parser shares.

mod directive;
mod facets;
mod filter;
mod fragment;
pub(crate) mod function;
mod godeep;
mod root;
mod schema;
pub(crate) mod variables;

use std::collections::{HashMap, HashSet};
use std::fmt::Display;

use ultragraphdb_core_keys::remove_duplicates;
use ultragraphdb_core_lex::{Item, ItemIterator, ItemType, Lexer};

use crate::errors::dql_error::DqlError;
use crate::state::{ITEM_LEFT_CURL, ITEM_NAME, ITEM_OP_TYPE, LEX_TOP_LEVEL};
use crate::types::fragment_node::FragmentNode;
use crate::types::parse_result::ParseResult;
use crate::types::request::Request;
use crate::types::vars::Vars;
use crate::utils::go_fmt::string_slice;

pub(crate) use self::variables::parse_var_name;

/// The parser's cursor over the lexed items.
pub(crate) type It<'l, 'a> = ItemIterator<'l, 'a>;

/// An error at an item's position, Go's `item.Errorf`.
pub(crate) fn ierr(item: &Item<'_>, msg: impl Display) -> DqlError {
    DqlError::from(item.errorf(msg))
}

/// An error at the position of the next item, Go's `it.Errorf`.
pub(crate) fn iterr(it: &It<'_, '_>, msg: impl Display) -> DqlError {
    DqlError::from(it.errorf(msg))
}

/// Go's `it.PeekOne()`: the next item, or the out-of-range item (type EOF, line and column -1) after the last.
pub(crate) fn peek_one_or_eof<'l, 'a>(it: &It<'l, 'a>) -> &'l Item<'a> {
    match it.peek_one() {
        Some(item) => item,
        None => {
            let mut before = it.clone();
            before.restore(-1);
            before.item()
        }
    }
}

/// Go's `it.Peek(1)`, its error as Dgraph's `Out of range for peek`.
pub(crate) fn peek1<'l, 'a>(it: &It<'l, 'a>) -> Result<&'l Item<'a>, DqlError> {
    Ok(&it.peek(1)?[0])
}

/// Steps over the next item when it has the type; the next item either way.
pub(crate) fn try_parse_item_type<'l, 'a>(
    it: &mut It<'l, 'a>,
    typ: ItemType,
) -> (&'l Item<'a>, bool) {
    let item = peek_one_or_eof(it);
    if it.peek_one().is_none() || item.typ() != typ {
        return (item, false);
    }
    it.next();
    (item, true)
}

/// Steps over the next item when it has the value.
pub(crate) fn try_skip_item_val(it: &mut It<'_, '_>, val: &str) -> bool {
    match it.peek_one() {
        Some(item) if item.val() == val => {
            it.next();
            true
        }
        _ => false,
    }
}

/// Steps over the next item when it has the type.
pub(crate) fn try_skip_item_typ(it: &mut It<'_, '_>, typ: ItemType) -> bool {
    try_parse_item_type(it, typ).1
}

/// A name can have dashes or alphanumeric characters, which the lexer lexes as separate items: put it back
/// together.
pub(crate) fn collect_name(it: &mut It<'_, '_>, val: &str) -> String {
    let mut val = val.to_string();
    // false if expecting dash, true if expecting name
    let mut dash_or_name = false;
    while let Some(next) = it.peek_one() {
        if (next.typ() == ITEM_NAME && dash_or_name) || (next.val() == "-" && !dash_or_name) {
            it.next();
            val.push_str(it.item().val());
            dash_or_name = !dash_or_name;
        } else {
            break;
        }
    }
    val
}

/// Parses a query: its blocks, fragments, parameters and schema block.
///
/// # Errors
///
/// Dgraph's lexer or parser error text.
pub fn parse(r: &Request) -> Result<ParseResult, DqlError> {
    parse_with_need_vars(r, &[])
}

/// Parses a query whose variables are also needed outside it: an upsert block's query defines the variables its
/// mutations use, which would otherwise count as defined but not used.
///
/// # Errors
///
/// Dgraph's lexer or parser error text.
pub fn parse_with_need_vars(r: &Request, need_vars: &[String]) -> Result<ParseResult, DqlError> {
    let mut vmap = variables::convert_to_var_map(&r.variables);
    let mut lexer = Lexer::new(&r.str);
    lexer.run(LEX_TOP_LEVEL);
    lexer.validate_result()?;

    let mut res = ParseResult::default();
    let mut it = lexer.new_iterator();
    let mut fmap: HashMap<String, FragmentNode> = HashMap::new();
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_OP_TYPE => match item.val() {
                "mutation" => return Err(ierr(item, "Mutation block no longer allowed.")),
                "schema" => {
                    if res.schema.is_some() {
                        return Err(ierr(item, "Only one schema block allowed "));
                    }
                    if !res.query.is_empty() {
                        return Err(ierr(item, "Schema block is not allowed with query block"));
                    }
                    res.schema = Some(schema::get_schema(&mut it)?);
                }
                "fragment" => {
                    let fnode = fragment::get_fragment(&mut it)?;
                    fmap.insert(fnode.name.clone(), fnode);
                }
                "query" => {
                    if res.schema.is_some() {
                        return Err(ierr(item, "Schema block is not allowed with query block"));
                    }
                    if let Some(qu) = variables::get_variables_and_query(&mut it, &mut vmap)? {
                        res.query.push(qu);
                    }
                }
                _ => {}
            },
            ITEM_LEFT_CURL => res.query.push(root::get_query(&mut it)?),
            ITEM_NAME => {
                it.prev();
                res.query.push(root::get_query(&mut it)?);
            }
            _ => {}
        }
    }

    if !res.query.is_empty() {
        res.query_vars = Vec::with_capacity(res.query.len());
        for qu in &mut res.query {
            // Try expanding fragments using fragment map.
            fragment::expand_fragments(qu, &mut fmap)?;
            // Substitute all DQL variables with corresponding values.
            variables::substitute_variables(qu, &vmap)?;
            // Collect vars used and defined in Result struct.
            let mut vars = Vars::default();
            qu.collect_vars(&mut vars);
            res.query_vars.push(vars);
        }
        // Add the variables that are needed outside the query block. For example, the mutation block of an
        // upsert block uses variables from the query block parsed here.
        let mut all_vars: Vec<&Vars> = res.query_vars.iter().collect();
        let outside = Vars {
            defines: Vec::new(),
            needs: need_vars.to_vec(),
        };
        if !need_vars.is_empty() {
            all_vars.push(&outside);
        }
        check_dependency(&all_vars)?;
    }
    validate_result(&res)?;
    Ok(res)
}

fn validate_result(res: &ParseResult) -> Result<(), DqlError> {
    let mut seen = HashSet::new();
    for q in &res.query {
        if q.alias == "var" || q.alias == "shortest" {
            continue;
        }
        if !seen.insert(q.alias.as_str()) {
            return Err(DqlError::parse(format!(
                "Duplicate aliases not allowed: {}",
                q.alias
            )));
        }
    }
    Ok(())
}

fn check_dependency(vl: &[&Vars]) -> Result<(), DqlError> {
    let needs = remove_duplicates(vl.iter().flat_map(|v| v.needs.iter().cloned()).collect());
    let defines: Vec<String> = vl.iter().flat_map(|v| v.defines.iter().cloned()).collect();
    let len_before = defines.len();
    let defines = remove_duplicates(defines);

    if defines.len() != len_before {
        return Err(DqlError::parse(
            "Some variables are declared multiple times.",
        ));
    }
    let detail = || {
        format!(
            "\nDefined:{}\nUsed:{}\n",
            string_slice(&defines),
            string_slice(&needs)
        )
    };
    if defines.len() > needs.len() {
        return Err(DqlError::parse(format!(
            "Some variables are defined but not used{}",
            detail()
        )));
    }
    if defines.len() < needs.len() {
        return Err(DqlError::parse(format!(
            "Some variables are used but not defined{}",
            detail()
        )));
    }
    if defines != needs {
        return Err(DqlError::parse(format!(
            "Variables are not used properly. {}",
            detail()
        )));
    }
    Ok(())
}

/// Splits `attr@en:ru` into the attribute and its languages.
pub(crate) fn attr_and_lang(attr_data: &str) -> (String, Vec<String>) {
    match attr_data.find('@') {
        None => (attr_data.to_string(), Vec::new()),
        Some(idx) => (
            attr_data[..idx].to_string(),
            attr_data[idx + 1..]
                .split(':')
                .map(str::to_string)
                .collect(),
        ),
    }
}
