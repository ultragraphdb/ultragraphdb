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

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_core_lex::Item;
use ultragraphdb_error_num::NumErrorEnum;

use super::variables::{parse_regex_args, parse_var_name, unquote_if_quoted};
use super::{
    It, check_depth, collect_name, ierr, iterr, peek_one_or_eof, try_parse_item_type,
    try_skip_item_typ,
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

/// A function being parsed, with the state Go's recursive `parseFunction` keeps in its locals.
struct Pending {
    function: Function,
    /// Whether the function's name and `(` were read, so that its arguments follow.
    started: bool,
    seen_func_arg: bool,
    expect_lang: bool,
    is_dollar: bool,
    similar_to_opt_seen: HashSet<String>,
    attr_items_ago: i32,
    expect_arg: bool,
}

/// Where parsing a function's items stopped.
enum Parsed<'l, 'a> {
    /// At the function's closing `)`, or at the end of the items.
    Done,
    /// At a function nested in its arguments, whose `(` this is; the iterator is back before the nested function's
    /// name.
    Nested(&'l Item<'a>),
}

impl Pending {
    fn new() -> Self {
        Self {
            function: Function::default(),
            started: false,
            seen_func_arg: false,
            expect_lang: false,
            is_dollar: false,
            similar_to_opt_seen: HashSet::new(),
            attr_items_ago: -1,
            expect_arg: true,
        }
    }

    /// Takes the result of the function nested in this one's arguments, whose `(` is `item_in_func`.
    fn take_nested(&mut self, nested: Function, item_in_func: &Item<'_>) -> Result<(), DqlError> {
        self.seen_func_arg = true;
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
                if self.function.attr.is_empty() {
                    // Variable is used in place of attribute, eq(val(a), 5)
                    self.function.attr = var;
                    self.function.is_value_var = true;
                } else {
                    // eq(name, val(a))
                    self.function.args.push(Arg {
                        value: var,
                        is_value_var: true,
                        is_dql_var: false,
                    });
                }
                self.function.needs_var.extend(nested.needs_var);
                if let Some(first) = self.function.needs_var.first_mut() {
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
                if !is_inequality_fn(&self.function.name) {
                    return Err(ierr(
                        item_in_func,
                        "len function only allowed inside inequality function",
                    ));
                }
                if !self.function.attr.is_empty() {
                    // eq(1, len(a))
                    return Err(ierr(
                        item_in_func,
                        "incorrect order, the first argument should be len function",
                    ));
                }
                // eq(len(a), 1)
                self.function.attr = nested
                    .needs_var
                    .first()
                    .map(|v| v.name.clone())
                    .unwrap_or_default();
                self.function.is_len_var = true;
                self.function.needs_var.extend(nested.needs_var);
            }
            "count" => {
                self.function.attr = nested.attr;
                self.function.is_count = true;
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
                self.function.needs_var.extend(nested.needs_var);
                self.function.needs_var[0].typ = VarType::Uid;
                self.function.args.push(Arg::new(var));
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
        self.expect_arg = false;
        Ok(())
    }
}

/// Parses a function whose node sits at `level`. `gq` is the block whose root function this is, which takes the
/// uids and the `$id` of a `uid` function; `None` inside filters.
///
/// Go's `parseFunction` recurses into a function nested in the arguments; this keeps the enclosing functions on a
/// stack instead, each nested function a level below the one holding it, and refuses nesting past
/// [`MAX_DEPTH`](super::MAX_DEPTH).
pub(crate) fn parse_function<'l, 'a>(
    it: &mut It<'l, 'a>,
    mut gq: Option<&mut GraphQuery>,
    level: usize,
) -> Result<Function, DqlError> {
    // The functions whose arguments hold the one being parsed, outermost first, each with the `(` of its nested
    // function.
    let mut enclosing: Vec<(Pending, &'l Item<'a>)> = Vec::new();
    let mut cur = Pending::new();
    loop {
        match parse_function_items(it, &mut cur, gq.as_deref_mut())? {
            Parsed::Nested(at) => {
                check_depth(level + enclosing.len() + 1)?;
                enclosing.push((std::mem::replace(&mut cur, Pending::new()), at));
            }
            Parsed::Done => {
                let function = complete(it, cur.function)?;
                match enclosing.pop() {
                    None => return Ok(function),
                    Some((mut outer, at)) => {
                        outer.take_nested(function, at)?;
                        cur = outer;
                    }
                }
            }
        }
    }
}

/// Parses the items of a function, its name and arguments, up to its closing `)` or a function nested in it.
fn parse_function_items<'l, 'a>(
    it: &mut It<'l, 'a>,
    st: &mut Pending,
    mut gq: Option<&mut GraphQuery>,
) -> Result<Parsed<'l, 'a>, DqlError> {
    loop {
        if !st.started {
            if !it.next() {
                return Ok(Parsed::Done);
            }
            let item = it.item();
            if item.typ() != ITEM_NAME {
                return Err(ierr(
                    item,
                    format!(
                        "Expected a function but got {}",
                        go_strconv::quote(item.val())
                    ),
                ));
            }
            let name = collect_name(it, item.val());
            st.function.name = name.to_lowercase();
            st.similar_to_opt_seen = HashSet::new();
            if !try_parse_item_type(it, ITEM_LEFT_ROUND).1 {
                return Err(iterr(
                    it,
                    format!("Expected ( after func name [{}]", st.function.name),
                ));
            }
            st.attr_items_ago = -1;
            st.expect_arg = true;
            st.started = true;
        }
        while it.next() {
            let mut item_in_func = it.item();
            if st.attr_items_ago >= 0 {
                st.attr_items_ago += 1;
            }
            let mut val = String::new();
            match item_in_func.typ() {
                ITEM_RIGHT_ROUND => return Ok(Parsed::Done),
                ITEM_COMMA => {
                    if st.expect_arg {
                        return Err(ierr(item_in_func, "Invalid use of comma."));
                    }
                    if st.is_dollar {
                        return Err(ierr(item_in_func, "Invalid use of comma after dollar."));
                    }
                    st.expect_arg = true;
                    continue;
                }
                ITEM_LEFT_ROUND => {
                    // Function inside a function.
                    if st.seen_func_arg {
                        return Err(ierr(
                            item_in_func,
                            "Multiple functions as arguments not allowed",
                        ));
                    }
                    it.prev();
                    it.prev();
                    return Ok(Parsed::Nested(item_in_func));
                }
                ITEM_AT => {
                    if st.attr_items_ago != 1 {
                        return Err(ierr(
                            item_in_func,
                            "Invalid usage of '@' in function argument, must only appear immediately after attr.",
                        ));
                    }
                    st.expect_lang = true;
                    continue;
                }
                ITEM_MATH_OP => {
                    val = item_in_func.val().to_string();
                    it.next();
                    item_in_func = it.item();
                }
                ITEM_DOLLAR => {
                    if st.is_dollar {
                        return Err(ierr(item_in_func, "Invalid use of $ in func args"));
                    }
                    st.is_dollar = true;
                    continue;
                }
                ITEM_REGEX => {
                    let ra = parse_regex_args(item_in_func.val())?;
                    st.function.args.push(Arg::new(ra.expr));
                    st.function.args.push(Arg::new(ra.flags));
                    st.expect_arg = false;
                    continue;
                }
                // Reassemble the geo tokens.
                ITEM_LEFT_SQUARE => {
                    if is_geo_func(&st.function.name) {
                        parse_geo_args(it, &mut st.function)?;
                    } else if is_inequality_fn(&st.function.name)
                        || st.function.name == "uid_in"
                        || st.function.name == SIMILAR_TO
                    {
                        parse_func_args(it, &mut st.function)?;
                    } else {
                        return Err(ierr(
                            item_in_func,
                            "Unexpected character [ while parsing request.",
                        ));
                    }
                    st.expect_arg = false;
                    continue;
                }
                ITEM_RIGHT_SQUARE => {
                    if it.peek(1).is_err() {
                        return Err(ierr(item_in_func, "Unexpected EOF while parsing args"));
                    }
                    st.expect_arg = false;
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
                        && st.function.name == SIMILAR_TO
                        && !st.function.attr.is_empty()
                        && st.function.args.len() >= 2
                    {
                        if it.peek_one().is_some_and(|next| next.typ() == ITEM_COLON) {
                            parse_similar_to_option(
                                it,
                                item_in_func,
                                &mut st.function,
                                &mut st.similar_to_opt_seen,
                            )?;
                            st.expect_arg = false;
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
                                st.function.name
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
            if !st.expect_arg && !st.expect_lang {
                return Err(ierr(
                    item_in_func,
                    format!("Expected comma or language but got: {}", item_in_func.val()),
                ));
            }

            let vname = collect_name(it, item_in_func.val());
            let v = unquote_if_quoted(vname.trim_matches([' ', '\t']))?;
            val.push_str(&v);

            if st.is_dollar {
                val = format!("${val}");
                st.is_dollar = false;
                match gq.as_deref_mut() {
                    Some(g) if st.function.name == "uid" => {
                        if g.args.get("id").is_some_and(|id| !id.is_empty()) {
                            return Err(ierr(
                                item_in_func,
                                "Only one DQL variable allowed inside uid function.",
                            ));
                        }
                        g.args.insert("id".to_string(), val);
                    }
                    _ => st.function.args.push(Arg {
                        value: val,
                        is_value_var: false,
                        is_dql_var: true,
                    }),
                }
                st.expect_arg = false;
                continue;
            }

            // Unlike other functions, uid function has no attribute, everything is args.
            if st.function.attr.is_empty()
                && st.function.name != "uid"
                && st.function.name != "type"
            {
                if item_in_func.val().contains('"') {
                    return Err(ierr(
                        item_in_func,
                        format!(
                            "Attribute in function must not be quoted with \": {}",
                            item_in_func.val()
                        ),
                    ));
                }
                if st.function.name == "uid_in" && next.typ() == ITEM_RIGHT_ROUND {
                    return Err(ierr(
                        item_in_func,
                        "uid_in function expects an argument, got none",
                    ));
                }
                st.function.attr = val.clone();
                st.attr_items_ago = 0;
            } else if st.expect_lang {
                if val == "*" {
                    return Err(DqlError::parse(
                        "The * symbol cannot be used as a valid language inside functions",
                    ));
                }
                st.function.lang = val.clone();
                st.expect_lang = false;
            } else if st.function.name != "uid" {
                // For UID function. we set g.UID
                st.function.args.push(Arg::new(val.clone()));
            }

            if st.function.name == "var" {
                return Err(ierr(
                    item_in_func,
                    "Unexpected var(). Maybe you want to try using uid()",
                ));
            }

            st.expect_arg = false;
            match st.function.name.as_str() {
                // E.g. @filter(gt(val(a), 10))
                "val" => st
                    .function
                    .needs_var
                    .push(VarContext::new(val, VarType::Value)),
                // E.g. @filter(gt(len(a), 10))
                "len" => st
                    .function
                    .needs_var
                    .push(VarContext::new(val, VarType::Uid)),
                "uid" => {
                    // uid function could take variables as well as actual uids. If we can parse the value that
                    // means it's a uid, otherwise a variable.
                    match go_strconv::parse_uint(&val, 0) {
                        Ok(uid) => {
                            match gq.as_deref_mut() {
                                // It could be uid function at root.
                                Some(g) => g.uid.push(uid),
                                // Or uid function in filter.
                                None => st.function.uid.push(uid),
                            }
                            continue;
                        }
                        Err(e) if matches!(e.0, NumErrorEnum::Range { .. }) => {
                            return Err(ierr(
                                item_in_func,
                                format!("The uid value {} is too large.", go_strconv::quote(&val)),
                            ));
                        }
                        // E.g. @filter(uid(a, b, c))
                        Err(_) => st
                            .function
                            .needs_var
                            .push(VarContext::new(val, VarType::Uid)),
                    }
                }
                _ => {}
            }
        }
        // The items ran out inside the arguments: Go's outer loop asks for another function name, past the end.
        st.started = false;
    }
}

/// The checks of a function once its arguments are parsed.
fn complete(it: &It<'_, '_>, function: Function) -> Result<Function, DqlError> {
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
            format!("Unknown option {} in similar_to", go_strconv::quote(&key)),
        ));
    }
    if !seen.insert(key.clone()) {
        return Err(ierr(
            item,
            format!(
                "Duplicate key {} in similar_to options",
                go_strconv::quote(&key)
            ),
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
