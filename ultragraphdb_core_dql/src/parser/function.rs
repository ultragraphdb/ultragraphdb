/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Functions: `eq(name, "x")`, nested `count`, `val`, `len` and `uid`, geo, list and regex arguments, and
//! `similar_to`'s named options.

use std::collections::HashSet;

use ultragraphdb_core_lex::Item;

use super::variables::{parse_regex_args, parse_var_name, unquote_if_quoted};
use super::{
    It, collect_name, ierr, iterr, peek_one_or_eof, try_parse_item_type, try_skip_item_typ,
};
use crate::errors::dql_error::DqlError;
use crate::state::{
    ITEM_AT, ITEM_COLON, ITEM_COMMA, ITEM_DOLLAR, ITEM_LEFT_CURL, ITEM_LEFT_ROUND,
    ITEM_LEFT_SQUARE, ITEM_MATH_OP, ITEM_NAME, ITEM_REGEX, ITEM_RIGHT_CURL, ITEM_RIGHT_ROUND,
    ITEM_RIGHT_SQUARE,
};
use crate::types::arg::Arg;
use crate::types::function::Function;
use crate::types::graph_query::GraphQuery;
use crate::types::var_context::VarContext;
use crate::types::var_type::VarType;
use crate::utils::go_fmt::arg_slice;
use crate::utils::go_quote::quote;
use crate::utils::go_strconv::parse_uint;

const SIMILAR_TO: &str = "similar_to";

pub(crate) fn is_geo_func(name: &str) -> bool {
    matches!(name, "near" | "contains" | "within" | "intersects")
}

/// Whether the function is an inequality: `eq`, `le`, `ge`, `gt`, `lt` or `between`.
pub fn is_inequality_fn(name: &str) -> bool {
    matches!(name, "eq" | "le" | "ge" | "gt" | "lt" | "between")
}

pub(crate) fn valid_func_name(name: &str) -> bool {
    is_geo_func(name)
        || is_inequality_fn(name)
        || matches!(
            name,
            "regexp"
                | "anyofterms"
                | "allofterms"
                | "alloftext"
                | "anyoftext"
                | "ngram"
                | "has"
                | "uid"
                | "uid_in"
                | "anyof"
                | "allof"
                | "type"
                | "match"
                | "similar_to"
        )
}

/// Reassembles the tokens of a geo argument, `[[1, 2], [3, 4]]`, into one argument.
fn parse_geo_args(it: &mut It<'_, '_>, g: &mut Function) -> Result<(), DqlError> {
    let mut buf = String::from("[");
    let mut depth = 1;
    loop {
        if !it.next() {
            return Err(iterr(it, "Got EOF while parsing Geo tokens"));
        }
        let item = it.item();
        match item.typ() {
            ITEM_LEFT_SQUARE => {
                buf.push_str(item.val());
                depth += 1;
            }
            ITEM_RIGHT_SQUARE => {
                buf.push_str(item.val());
                depth -= 1;
            }
            // Writing tokens to buffer.
            ITEM_MATH_OP | ITEM_COMMA | ITEM_NAME => buf.push_str(item.val()),
            _ => {
                return Err(ierr(
                    item,
                    format!(
                        "Found invalid item: {} while parsing geo arguments.",
                        item.val()
                    ),
                ));
            }
        }
        if !(0..=4).contains(&depth) {
            return Err(ierr(item, "Invalid bracket sequence"));
        }
        if depth == 0 {
            break;
        }
    }
    // Append the concatenated Geo token to Args.
    g.args.push(Arg::new(buf));
    let Ok(items) = it.peek(1) else {
        return Err(iterr(it, "Unexpected EOF while parsing args"));
    };
    let item = &items[0];
    if item.typ() != ITEM_RIGHT_ROUND && item.typ() != ITEM_COMMA {
        return Err(ierr(
            item,
            format!("Expected right round or comma. Got: {item}"),
        ));
    }
    Ok(())
}

/// Parses the arguments inside an array (`[]`). Values with a `$` are query parameters, expanded later; others
/// are scalar values.
fn parse_func_args(it: &mut It<'_, '_>, g: &mut Function) -> Result<(), DqlError> {
    let mut expect_arg = true;
    let mut is_dollar = false;
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_RIGHT_SQUARE => return Ok(()),
            ITEM_DOLLAR => {
                if !expect_arg {
                    return Err(ierr(item, "Missing comma in argument list declaration"));
                }
                let next = peek_one_or_eof(it);
                if it.peek_one().is_none() || next.typ() != ITEM_NAME {
                    return Err(ierr(
                        next,
                        format!("Expecting a variable name. Got: {next}"),
                    ));
                }
                is_dollar = true;
                continue;
            }
            ITEM_NAME => {
                if is_dollar {
                    // This is a $variable that must be expanded later.
                    g.args.push(Arg {
                        value: format!("${}", item.val()),
                        is_value_var: false,
                        is_dql_var: true,
                    });
                } else {
                    // This is not a $variable, just add the value.
                    g.args.push(Arg::new(get_value_arg(item.val())?));
                }
            }
            ITEM_COMMA => {
                if expect_arg {
                    return Err(ierr(item, "Invalid comma in argument list"));
                }
                expect_arg = true;
                continue;
            }
            _ => return Err(ierr(item, "Invalid arg list")),
        }
        expect_arg = false;
        is_dollar = false;
    }
    Err(iterr(
        it,
        format!(
            "Expecting ] to end list but got {} instead",
            it.item().val()
        ),
    ))
}

/// A space-trimmed and unquoted version of the value.
fn get_value_arg(val: &str) -> Result<String, DqlError> {
    unquote_if_quoted(val.trim_matches(|c: char| c.is_whitespace()))
}

/// Parses a function. `gq` is the block whose root function this is, which takes the uids and the `$id` of a
/// `uid` function; `None` inside filters.
pub(crate) fn parse_function(
    it: &mut It<'_, '_>,
    mut gq: Option<&mut GraphQuery>,
) -> Result<Function, DqlError> {
    let mut function = Function::default();
    let mut seen_func_arg = false;
    let mut expect_lang = false;
    let mut is_dollar = false;
    'outer: while it.next() {
        let item = it.item();
        if item.typ() != ITEM_NAME {
            return Err(ierr(
                item,
                format!("Expected a function but got {}", quote(item.val())),
            ));
        }
        let name = collect_name(it, item.val());
        function.name = name.to_lowercase();
        let mut similar_to_opt_seen: HashSet<String> = HashSet::new();
        if !try_parse_item_type(it, ITEM_LEFT_ROUND).1 {
            return Err(iterr(
                it,
                format!("Expected ( after func name [{}]", function.name),
            ));
        }

        let mut attr_items_ago: i32 = -1;
        let mut expect_arg = true;
        while it.next() {
            let mut item_in_func = it.item();
            if attr_items_ago >= 0 {
                attr_items_ago += 1;
            }
            let mut val = String::new();
            match item_in_func.typ() {
                ITEM_RIGHT_ROUND => break 'outer,
                ITEM_COMMA => {
                    if expect_arg {
                        return Err(ierr(item_in_func, "Invalid use of comma."));
                    }
                    if is_dollar {
                        return Err(ierr(item_in_func, "Invalid use of comma after dollar."));
                    }
                    expect_arg = true;
                    continue;
                }
                ITEM_LEFT_ROUND => {
                    // Function inside a function.
                    if seen_func_arg {
                        return Err(ierr(
                            item_in_func,
                            "Multiple functions as arguments not allowed",
                        ));
                    }
                    it.prev();
                    it.prev();
                    let nested = parse_function(it, gq.as_deref_mut())?;
                    seen_func_arg = true;
                    match nested.name.as_str() {
                        "val" => {
                            if nested.needs_var.len() > 1 {
                                return Err(ierr(
                                    item_in_func,
                                    "Multiple variables not allowed in a function",
                                ));
                            }
                            let var = nested
                                .needs_var
                                .first()
                                .map(|v| v.name.clone())
                                .unwrap_or_default();
                            if function.attr.is_empty() {
                                // Variable is used in place of attribute, eq(val(a), 5)
                                function.attr = var;
                                function.is_value_var = true;
                            } else {
                                // eq(name, val(a))
                                function.args.push(Arg {
                                    value: var,
                                    is_value_var: true,
                                    is_dql_var: false,
                                });
                            }
                            function.needs_var.extend(nested.needs_var);
                            if let Some(first) = function.needs_var.first_mut() {
                                first.typ = VarType::Value;
                            }
                        }
                        "len" => {
                            if nested.needs_var.len() > 1 {
                                return Err(ierr(
                                    item_in_func,
                                    "Multiple variables not allowed in len function",
                                ));
                            }
                            if !is_inequality_fn(&function.name) {
                                return Err(ierr(
                                    item_in_func,
                                    "len function only allowed inside inequality function",
                                ));
                            }
                            if !function.attr.is_empty() {
                                // eq(1, len(a))
                                return Err(ierr(
                                    item_in_func,
                                    "incorrect order, the first argument should be len function",
                                ));
                            }
                            // eq(len(a), 1)
                            function.attr = nested
                                .needs_var
                                .first()
                                .map(|v| v.name.clone())
                                .unwrap_or_default();
                            function.is_len_var = true;
                            function.needs_var.extend(nested.needs_var);
                        }
                        "count" => {
                            function.attr = nested.attr;
                            function.is_count = true;
                        }
                        "uid" => {
                            if nested.needs_var.len() != 1 {
                                return Err(ierr(
                                    item_in_func,
                                    format!(
                                        "Nested uid fn expects 1 uid variable, got {}",
                                        nested.needs_var.len()
                                    ),
                                ));
                            }
                            if !nested.uid.is_empty() {
                                return Err(ierr(
                                    item_in_func,
                                    "Nested uid fn expects only uid variable, got UID",
                                ));
                            }
                            let var = nested.needs_var[0].name.clone();
                            function.needs_var.extend(nested.needs_var);
                            function.needs_var[0].typ = VarType::Uid;
                            function.args.push(Arg::new(var));
                        }
                        other => {
                            return Err(ierr(
                                item_in_func,
                                format!(
                                    "Only val/count/len/uid allowed as function within another. Got: {other}"
                                ),
                            ));
                        }
                    }
                    expect_arg = false;
                    continue;
                }
                ITEM_AT => {
                    if attr_items_ago != 1 {
                        return Err(ierr(
                            item_in_func,
                            "Invalid usage of '@' in function argument, must only appear immediately after attr.",
                        ));
                    }
                    expect_lang = true;
                    continue;
                }
                ITEM_MATH_OP => {
                    val = item_in_func.val().to_string();
                    it.next();
                    item_in_func = it.item();
                }
                ITEM_DOLLAR => {
                    if is_dollar {
                        return Err(ierr(item_in_func, "Invalid use of $ in func args"));
                    }
                    is_dollar = true;
                    continue;
                }
                ITEM_REGEX => {
                    let ra = parse_regex_args(item_in_func.val())?;
                    function.args.push(Arg::new(ra.expr));
                    function.args.push(Arg::new(ra.flags));
                    expect_arg = false;
                    continue;
                }
                // Reassemble the geo tokens.
                ITEM_LEFT_SQUARE => {
                    if is_geo_func(&function.name) {
                        parse_geo_args(it, &mut function)?;
                    } else if is_inequality_fn(&function.name)
                        || function.name == "uid_in"
                        || function.name == SIMILAR_TO
                    {
                        parse_func_args(it, &mut function)?;
                    } else {
                        return Err(ierr(
                            item_in_func,
                            "Unexpected character [ while parsing request.",
                        ));
                    }
                    expect_arg = false;
                    continue;
                }
                ITEM_RIGHT_SQUARE => {
                    if it.peek(1).is_err() {
                        return Err(ierr(item_in_func, "Unexpected EOF while parsing args"));
                    }
                    expect_arg = false;
                    continue;
                }
                ITEM_LEFT_CURL => {
                    return Err(ierr(
                        item_in_func,
                        "Unrecognized character inside a func: U+007B '{'",
                    ));
                }
                ITEM_RIGHT_CURL => {
                    // Right curly braces are never valid in function arguments.
                    return Err(ierr(
                        item_in_func,
                        "Unrecognized character inside a func: U+007D '}'",
                    ));
                }
                _ => {
                    // similar_to takes named options after its positional arguments:
                    // similar_to(pred, k, vec, ef: 64, distance_threshold: 0.5), held as two args each after k and
                    // vec: ["ef", "64", "distance_threshold", "0.5", ...].
                    if item_in_func.typ() == ITEM_NAME
                        && function.name == SIMILAR_TO
                        && !function.attr.is_empty()
                        && function.args.len() >= 2
                    {
                        if it.peek_one().is_some_and(|next| next.typ() == ITEM_COLON) {
                            parse_similar_to_option(
                                it,
                                item_in_func,
                                &mut function,
                                &mut similar_to_opt_seen,
                            )?;
                            expect_arg = false;
                            continue;
                        }
                        // Disallow extra positional args after (k, vec). Options must be named.
                        return Err(ierr(
                            item_in_func,
                            "Expected named parameter in similar_to options (e.g. ef: 64)",
                        ));
                    }
                    if item_in_func.typ() != ITEM_NAME {
                        return Err(ierr(
                            item_in_func,
                            format!(
                                "Expected arg after func [{}], but got item {item_in_func}",
                                function.name
                            ),
                        ));
                    }
                }
            }

            let next = peek_one_or_eof(it);
            if it.peek_one().is_none() {
                return Err(ierr(next, format!("Unexpected item: {next}")));
            }
            // Part of function continue
            if next.typ() == ITEM_LEFT_ROUND {
                continue;
            }
            if !expect_arg && !expect_lang {
                return Err(ierr(
                    item_in_func,
                    format!("Expected comma or language but got: {}", item_in_func.val()),
                ));
            }

            let vname = collect_name(it, item_in_func.val());
            let v = unquote_if_quoted(vname.trim_matches([' ', '\t']))?;
            val.push_str(&v);

            if is_dollar {
                val = format!("${val}");
                is_dollar = false;
                match gq.as_deref_mut() {
                    Some(g) if function.name == "uid" => {
                        if g.args.get("id").is_some_and(|id| !id.is_empty()) {
                            return Err(ierr(
                                item_in_func,
                                "Only one DQL variable allowed inside uid function.",
                            ));
                        }
                        g.args.insert("id".to_string(), val);
                    }
                    _ => function.args.push(Arg {
                        value: val,
                        is_value_var: false,
                        is_dql_var: true,
                    }),
                }
                expect_arg = false;
                continue;
            }

            // Unlike other functions, uid function has no attribute, everything is args.
            if function.attr.is_empty() && function.name != "uid" && function.name != "type" {
                if item_in_func.val().contains('"') {
                    return Err(ierr(
                        item_in_func,
                        format!(
                            "Attribute in function must not be quoted with \": {}",
                            item_in_func.val()
                        ),
                    ));
                }
                if function.name == "uid_in" && next.typ() == ITEM_RIGHT_ROUND {
                    return Err(ierr(
                        item_in_func,
                        "uid_in function expects an argument, got none",
                    ));
                }
                function.attr = val.clone();
                attr_items_ago = 0;
            } else if expect_lang {
                if val == "*" {
                    return Err(DqlError::parse(
                        "The * symbol cannot be used as a valid language inside functions",
                    ));
                }
                function.lang = val.clone();
                expect_lang = false;
            } else if function.name != "uid" {
                // For UID function. we set g.UID
                function.args.push(Arg::new(val.clone()));
            }

            if function.name == "var" {
                return Err(ierr(
                    item_in_func,
                    "Unexpected var(). Maybe you want to try using uid()",
                ));
            }

            expect_arg = false;
            match function.name.as_str() {
                // E.g. @filter(gt(val(a), 10))
                "val" => function
                    .needs_var
                    .push(VarContext::new(val, VarType::Value)),
                // E.g. @filter(gt(len(a), 10))
                "len" => function.needs_var.push(VarContext::new(val, VarType::Uid)),
                "uid" => {
                    // uid function could take variables as well as actual uids. If we can parse the value that
                    // means it's a uid, otherwise a variable.
                    match parse_uint(&val, 0) {
                        Ok(uid) => {
                            match gq.as_deref_mut() {
                                // It could be uid function at root.
                                Some(g) => g.uid.push(uid),
                                // Or uid function in filter.
                                None => function.uid.push(uid),
                            }
                            continue;
                        }
                        Err(e) if e.range => {
                            return Err(ierr(
                                item_in_func,
                                format!("The uid value {} is too large.", quote(&val)),
                            ));
                        }
                        // E.g. @filter(uid(a, b, c))
                        Err(_) => function.needs_var.push(VarContext::new(val, VarType::Uid)),
                    }
                }
                _ => {}
            }
        }
    }

    if function.name != "uid" && function.name != "type" && function.attr.is_empty() {
        return Err(iterr(
            it,
            format!("Got empty attr for function: [{}]", function.name),
        ));
    }
    if function.name == "type" && function.args.len() != 1 {
        return Err(iterr(
            it,
            format!(
                "type function only supports one argument. Got: {}",
                arg_slice(&function.args)
            ),
        ));
    }
    Ok(function)
}

/// Parses one named option of `similar_to`, `ef: 64` or `distance_threshold: $d`, whose name is the current
/// item.
fn parse_similar_to_option(
    it: &mut It<'_, '_>,
    item: &Item<'_>,
    function: &mut Function,
    seen: &mut HashSet<String>,
) -> Result<(), DqlError> {
    let key = collect_name(it, item.val()).to_lowercase();
    if key != "ef" && key != "distance_threshold" {
        return Err(ierr(
            item,
            format!("Unknown option {} in similar_to", quote(&key)),
        ));
    }
    if !seen.insert(key.clone()) {
        return Err(ierr(
            item,
            format!("Duplicate key {} in similar_to options", quote(&key)),
        ));
    }
    if !try_skip_item_typ(it, ITEM_COLON) {
        return Err(iterr(it, format!("Expected colon(:) after {key}")));
    }
    if !it.next() {
        return Err(iterr(it, format!("Expected value for {key}")));
    }
    let mut val_item = it.item();
    let value = match val_item.typ() {
        ITEM_DOLLAR => {
            let var_name = parse_var_name(it)?;
            function.args.push(Arg::new(key));
            function.args.push(Arg {
                value: var_name,
                is_value_var: false,
                is_dql_var: true,
            });
            return Ok(());
        }
        ITEM_MATH_OP => {
            // Allow signed numeric literals, e.g. distance_threshold: -0.5
            let prefix = val_item.val();
            if !it.next() {
                return Err(iterr(
                    it,
                    format!("Expected value after {prefix} for {key}"),
                ));
            }
            val_item = it.item();
            if val_item.typ() != ITEM_NAME {
                return Err(ierr(val_item, format!("Expected value for {key}")));
            }
            let v = collect_name(it, val_item.val());
            let uq = unquote_if_quoted(v.trim_matches([' ', '\t']))?;
            format!("{prefix}{uq}")
        }
        _ => {
            if val_item.typ() != ITEM_NAME {
                return Err(ierr(val_item, format!("Expected value for {key}")));
            }
            let v = collect_name(it, val_item.val());
            unquote_if_quoted(v.trim_matches([' ', '\t']))?
        }
    };
    function.args.push(Arg::new(key));
    function.args.push(Arg::new(value));
    Ok(())
}
