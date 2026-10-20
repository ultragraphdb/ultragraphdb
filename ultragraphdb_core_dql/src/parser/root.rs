/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! A query block's root: its name, arguments and root directives.

use std::collections::{BTreeMap, HashSet};

use ultragraphdb_proto_pb::pb;

use super::directive::{parse_cascade, parse_groupby, parse_language_list};
use super::filter::parse_filter;
use super::function::{parse_function, valid_func_name};
use super::godeep::godeep;
use super::variables::{parse_var_list, parse_var_name};
use super::{
    It, attr_and_lang, collect_name, ierr, iterr, peek1, try_parse_item_type, try_skip_item_typ,
};
use crate::errors::dql_error::DqlError;
use crate::state::{
    ITEM_AT, ITEM_COLON, ITEM_COMMA, ITEM_DOLLAR, ITEM_LEFT_CURL, ITEM_LEFT_ROUND, ITEM_MATH_OP,
    ITEM_NAME, ITEM_RIGHT_CURL, ITEM_RIGHT_ROUND,
};
use crate::types::function::Function;
use crate::types::graph_query::GraphQuery;
use crate::types::var_type::VarType;
use crate::utils::go_quote::quote;
use crate::utils::go_strconv::{parse_bool, parse_uint};

/// Parses the arguments of `@recurse(depth: 2, loop: true)`, when it has any.
fn parse_recurse_args(it: &mut It<'_, '_>, gq: &mut GraphQuery) -> Result<(), DqlError> {
    if !try_skip_item_typ(it, ITEM_LEFT_ROUND) {
        // We don't have a (, we can return.
        return Ok(());
    }
    while it.next() {
        let mut item = it.item();
        if item.typ() != ITEM_NAME {
            return Err(ierr(item, "Expected key inside @recurse()"));
        }
        let key = item.val().to_lowercase();
        if !try_skip_item_typ(it, ITEM_COLON) {
            return Err(iterr(it, format!("Expected colon(:) after {key}")));
        }
        if !it.next() {
            return Err(iterr(it, "Expected argument"));
        }
        // Consume the next item.
        item = it.item();
        let val = item.val();
        match key.as_str() {
            "depth" | "loop" if item.typ() == ITEM_DOLLAR => {
                // Consume the variable name.
                let var_name = parse_var_name(it)?;
                gq.recurse_args
                    .var_map
                    .get_or_insert_with(BTreeMap::new)
                    .insert(key.clone(), var_name);
            }
            "depth" => {
                if item.typ() != ITEM_NAME {
                    return Err(ierr(
                        item,
                        format!("Expected value inside @recurse() for key: {key}"),
                    ));
                }
                gq.recurse_args.depth = parse_uint(val, 0)
                    .map_err(|_| DqlError::parse("Value inside depth should be type of integer"))?;
            }
            "loop" => {
                gq.recurse_args.allow_loop = parse_bool(val)
                    .map_err(|_| DqlError::parse("Value inside loop should be type of boolean"))?;
            }
            _ => {
                return Err(ierr(
                    item,
                    format!("Unexpected key: [{key}] inside @recurse block"),
                ));
            }
        }
        if try_parse_item_type(it, ITEM_RIGHT_ROUND).1 {
            return Ok(());
        }
        if !try_parse_item_type(it, ITEM_COMMA).1 {
            return Err(iterr(
                it,
                format!("Expected comma after value: {val} inside recurse block"),
            ));
        }
    }
    Ok(())
}

/// Parses a query block: its root, its root directives and, through [`godeep`], its children.
pub(crate) fn get_query(it: &mut It<'_, '_>) -> Result<GraphQuery, DqlError> {
    // First, get the root.
    let mut gq = get_root(it)?;
    let mut seen_filter = false;
    loop {
        // Recurse to deeper levels through godeep.
        if !it.next() {
            return Err(iterr(it, "Expecting more lexer items while parsing query"));
        }
        let item = it.item();
        match item.typ() {
            ITEM_LEFT_CURL => godeep(it, &mut gq)?,
            ITEM_AT => {
                it.next();
                let item = it.item();
                if item.typ() == ITEM_NAME {
                    match item.val().to_lowercase().as_str() {
                        "filter" => {
                            if seen_filter {
                                return Err(ierr(item, "Repeated filter at root"));
                            }
                            seen_filter = true;
                            gq.filter = parse_filter(it)?;
                        }
                        "normalize" => gq.normalize = true,
                        "cascade" => parse_cascade(it, &mut gq)?,
                        "groupby" => {
                            gq.is_groupby = true;
                            parse_groupby(it, &mut gq)?;
                        }
                        "ignorereflex" => gq.ignore_reflex = true,
                        "recurse" => {
                            gq.recurse = true;
                            parse_recurse_args(it, &mut gq)?;
                        }
                        _ => return Err(ierr(item, format!("Unknown directive [{}]", item.val()))),
                    }
                    continue;
                }
            }
            // Do nothing.
            ITEM_RIGHT_CURL => {}
            ITEM_NAME => {
                it.prev();
                return Ok(gq);
            }
            _ => {
                return Err(ierr(
                    item,
                    format!("Malformed Query. Missing {{. Got {}", item.val()),
                ));
            }
        }
        return Ok(gq);
    }
}

fn valid_key_at_root(k: &str) -> bool {
    matches!(
        k,
        "func"
            | "orderasc"
            | "orderdesc"
            | "first"
            | "offset"
            | "after"
            // Specific to shortest path
            | "from"
            | "to"
            | "numpaths"
            | "minweight"
            | "maxweight"
            | "maxfrontiersize"
            | "depth"
    )
}

pub(crate) fn is_sortkey(k: &str) -> bool {
    k == "orderasc" || k == "orderdesc"
}

fn is_empty(gq: &GraphQuery) -> bool {
    gq.func.is_none()
        && gq.needs_var.is_empty()
        && gq.args.is_empty()
        && gq.shortest_path_args.from.is_none()
        && gq.shortest_path_args.to.is_none()
}

/// Parses the root of a block, `name as alias(func: ..., first: 10)`.
fn get_root(it: &mut It<'_, '_>) -> Result<GraphQuery, DqlError> {
    let mut gq = GraphQuery::default();
    if !it.next() {
        return Err(iterr(it, "Invalid query"));
    }
    let mut item = it.item();
    if item.typ() != ITEM_NAME {
        return Err(ierr(item, format!("Expected some name. Got: {item}")));
    }
    let Ok(peek) = it.peek(1) else {
        return Err(iterr(it, "Invalid Query"));
    };
    if peek[0].typ() == ITEM_NAME && peek[0].val().to_lowercase() == "as" {
        gq.var = item.val().to_string();
        it.next(); // Consume the "AS".
        it.next();
        item = it.item();
    }
    gq.alias = item.val().to_string();
    if !it.next() {
        return Err(ierr(item, "Invalid query"));
    }
    item = it.item();
    if item.typ() != ITEM_LEFT_ROUND {
        return Err(ierr(
            item,
            format!("Expected Left round brackets. Got: {item}"),
        ));
    }

    let mut expect_arg = true;
    let mut order: HashSet<String> = HashSet::new();
    // Parse in KV fashion. Depending on the value of key, decide the path.
    while it.next() {
        // Get key.
        let mut item = it.item();
        let key = match item.typ() {
            ITEM_NAME => {
                if !expect_arg {
                    return Err(ierr(item, format!("Not expecting argument. Got: {item}")));
                }
                expect_arg = false;
                item.val().to_string()
            }
            ITEM_RIGHT_ROUND => {
                if is_empty(&gq) {
                    // Used to do aggregation at root which would be fetched in another block.
                    gq.is_empty = true;
                }
                break;
            }
            ITEM_COMMA => {
                if expect_arg {
                    return Err(ierr(item, "Expected Argument but got comma."));
                }
                expect_arg = true;
                continue;
            }
            _ => return Err(ierr(item, format!("Expecting argument name. Got: {item}"))),
        };
        if !valid_key_at_root(&key) {
            return Err(ierr(item, format!("Got invalid keyword: {key} at root")));
        }
        if !it.next() {
            return Err(ierr(item, "Invalid query"));
        }
        item = it.item();
        if item.typ() != ITEM_COLON {
            return Err(ierr(item, format!("Expecting a colon. Got: {item}")));
        }

        match key.as_str() {
            "func" => {
                // Store the generator function.
                if gq.func.is_some() {
                    return Err(ierr(item, "Only one function allowed at root"));
                }
                let gen_fn = parse_function(it, Some(&mut gq))?;
                if !valid_func_name(&gen_fn.name) {
                    return Err(ierr(
                        item,
                        format!("Function name: {} is not valid.", gen_fn.name),
                    ));
                }
                gq.needs_var.extend(gen_fn.needs_var.iter().cloned());
                gq.func = Some(gen_fn);
            }
            "from" | "to" => {
                if gq.alias != "shortest" {
                    return Err(ierr(item, "from/to only allowed for shortest path queries"));
                }
                let mut f = Function::default();
                let Ok(peek) = it.peek(1) else {
                    return Err(ierr(item, "Invalid query"));
                };
                if peek[0].val() == "uid" {
                    let gen_fn = parse_function(it, Some(&mut gq))?;
                    f.needs_var = gen_fn.needs_var;
                    f.name = gen_fn.name;
                } else {
                    // This means it's not a uid function, so it has to be an actual uid.
                    it.next();
                    let item = it.item();
                    let val = collect_name(it, item.val());
                    match parse_uint(&val, 0) {
                        Ok(uid) => f.uid.push(uid),
                        Err(e) if e.range => {
                            return Err(ierr(
                                item,
                                format!("The uid value {} is too large.", quote(&val)),
                            ));
                        }
                        Err(_) => {
                            return Err(ierr(
                                item,
                                format!(
                                    "from/to in shortest path can only accept uid function or an uid. Got: {val}"
                                ),
                            ));
                        }
                    }
                }
                if key == "from" {
                    gq.shortest_path_args.from = Some(f);
                } else {
                    gq.shortest_path_args.to = Some(f);
                }
            }
            _ => {
                let mut val = String::new();
                if !it.next() {
                    return Err(iterr(it, "Invalid query"));
                }
                let mut item = it.item();
                let mut assign_only = false;
                if item.typ() == ITEM_DOLLAR {
                    it.next();
                    item = it.item();
                    if item.typ() != ITEM_NAME {
                        return Err(ierr(
                            item,
                            format!("Expecting a variable name. Got: {item}"),
                        ));
                    }
                    val = format!("${}", item.val());
                    assign_only = true;
                } else if item.typ() == ITEM_MATH_OP {
                    if item.val() != "+" && item.val() != "-" {
                        return Err(ierr(
                            item,
                            format!(
                                "Only Plus and minus are allowed unary ops. Got: {}",
                                item.val()
                            ),
                        ));
                    }
                    val = item.val().to_string();
                    it.next();
                    item = it.item();
                }

                if !assign_only {
                    if val.is_empty() && item.val() == "val" {
                        let count = parse_var_list(it, &mut gq.needs_var)?;
                        if count != 1 {
                            return Err(ierr(
                                item,
                                format!("Expected only one variable but got: {count}"),
                            ));
                        }
                        // Modify the NeedsVar context here.
                        if let Some(last) = gq.needs_var.last_mut() {
                            last.typ = VarType::Value;
                        }
                    } else {
                        val = collect_name(it, &format!("{val}{}", item.val()));
                        // Get language list, if present
                        if let Ok(next) = peek1(it) {
                            if next.typ() == ITEM_LEFT_ROUND && is_sortkey(&key) && val != "val" {
                                return Err(iterr(
                                    it,
                                    format!("Expected val(). Got {val}() with order."),
                                ));
                            }
                            if next.typ() == ITEM_AT {
                                it.next(); // consume '@'
                                it.next(); // move forward
                                let langs = parse_language_list(it)?;
                                val = format!("{val}@{}", langs.join(":"));
                            }
                        }
                    }

                    // Allow only order by one of variable/predicate for now.
                    if val.is_empty() {
                        // This should only happen in cases like: orderasc: val(c)
                        let Some(last) = gq.needs_var.last() else {
                            return Err(iterr(
                                it,
                                "unable to get value when parsing key value pairs",
                            ));
                        };
                        val = last.name.clone();
                        // Right now we only allow one sort by a variable
                        if !gq.order.is_empty() && is_sortkey(&key) {
                            return Err(iterr(
                                it,
                                format!("Multiple sorting only allowed by predicates. Got: {val}"),
                            ));
                        }
                    }
                    if is_sortkey(&key) {
                        // we dont support variable + predicate sorting
                        for o in &gq.order {
                            if let Some(nv) = gq.needs_var.iter().find(|nv| nv.name == o.attr) {
                                return Err(iterr(
                                    it,
                                    format!(
                                        "Val() is not allowed in multiple sorting. Got: [{}]",
                                        nv.name
                                    ),
                                ));
                            }
                        }
                        if order.contains(&val) {
                            return Err(iterr(
                                it,
                                format!("Sorting by an attribute: [{val}] can only be done once"),
                            ));
                        }
                        let (attr, langs) = attr_and_lang(&val);
                        if langs.len() > 1 {
                            return Err(iterr(
                                it,
                                format!(
                                    "Sorting by an attribute: [{val}] can only be done on one language"
                                ),
                            ));
                        }
                        gq.order.push(pb::Order {
                            attr,
                            desc: key == "orderdesc",
                            langs,
                        });
                        order.insert(val);
                        continue;
                    }
                }
                if gq.args.contains_key(&key) {
                    return Err(iterr(it, format!("Repeated key {} at root", quote(&key))));
                }
                gq.args.insert(key, val);
            }
        }
    }
    Ok(gq)
}
