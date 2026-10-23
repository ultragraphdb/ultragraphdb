/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Query parameters: their declaration, type checks and substitution; and variable lists.

use std::collections::{BTreeMap, HashMap};

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_core_types::{TypeId, Val, Value, parse_vfloat};

use super::{It, ierr, iterr, root};
use crate::errors::dql_error::DqlError;
use crate::math::subs;
use crate::state::{
    ITEM_COLON, ITEM_COMMA, ITEM_DOLLAR, ITEM_EQUAL, ITEM_LEFT_CURL, ITEM_LEFT_ROUND, ITEM_MATH_OP,
    ITEM_NAME, ITEM_RIGHT_ROUND,
};
use crate::types::arg::Arg;
use crate::types::filter_tree::FilterTree;
use crate::types::function::Function;
use crate::types::graph_query::GraphQuery;
use crate::types::regex_args::RegexArgs;
use crate::types::var_context::VarContext;
use crate::types::var_info::VarInfo;
use crate::types::var_type::VarType;

/// The query parameters by name, with the `$`.
pub(crate) type VarMap = BTreeMap<String, VarInfo>;

pub(crate) fn convert_to_var_map(variables: &HashMap<String, String>) -> VarMap {
    variables
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                VarInfo {
                    value: v.clone(),
                    typ: String::new(),
                },
            )
        })
        .collect()
}

/// The typed value of a query parameter, for a math expression.
pub(crate) fn parse_value(v: &VarInfo) -> Result<Val, DqlError> {
    if v.value.is_empty() {
        return Err(DqlError::parse("No value found"));
    }
    let wrap = |what: &str, e: &dyn std::fmt::Display| {
        DqlError::parse(format!("Expected {what} but got {}: {e}", v.value))
    };
    match v.typ.as_str() {
        "int" => go_strconv::parse_int(&v.value, 0)
            .map(|i| Val::new(TypeId::INT, Value::Int(i)))
            .map_err(|e| wrap("an int", &e)),
        "float" => go_strconv::parse_float(&v.value, 64)
            .map(|f| Val::new(TypeId::FLOAT, Value::Float(f)))
            .map_err(|e| wrap("a float", &e)),
        "bool" => go_strconv::parse_bool(&v.value)
            .map(|b| Val::new(TypeId::BOOL, Value::Bool(b)))
            .map_err(|e| wrap("a bool", &e)),
        "float32vector" => parse_vfloat(&v.value)
            .map(|f| Val::new(TypeId::VFLOAT, Value::VFloat(f)))
            .map_err(|e| wrap("a float32vector", &e)),
        // Value is a valid string. No checks required.
        "string" => Ok(Val::new(TypeId::STRING, Value::str(&v.value))),
        typ => Err(DqlError::parse(format!(
            "Type {} not supported",
            go_strconv::quote(typ)
        ))),
    }
}

/// Checks the declared types of the query parameters, and the values given against them.
pub(crate) fn check_value_type(vm: &VarMap) -> Result<(), DqlError> {
    for (k, v) in vm {
        let mut typ = v.typ.as_str();
        if typ.is_empty() {
            return Err(DqlError::parse(format!(
                "Type of variable {k} not specified"
            )));
        }
        // Ensure value is not nil if the variable is required.
        if let Some(t) = typ.strip_suffix('!') {
            if v.value.is_empty() {
                return Err(DqlError::parse(format!(
                    "Variable {k} should be initialised"
                )));
            }
            typ = t;
        }
        // Type check the values.
        if !v.value.is_empty() {
            let wrap = |what: &str, e: &dyn std::fmt::Display| {
                DqlError::parse(format!("Expected {what} but got {}: {e}", v.value))
            };
            match typ {
                "int" => {
                    go_strconv::parse_int(&v.value, 0).map_err(|e| wrap("an int", &e))?;
                }
                "float" => {
                    go_strconv::parse_float(&v.value, 64).map_err(|e| wrap("a float", &e))?;
                }
                "bool" => {
                    go_strconv::parse_bool(&v.value).map_err(|e| wrap("a bool", &e))?;
                }
                "float32vector" => {
                    parse_vfloat(&v.value).map_err(|e| wrap("a vector32float", &e))?;
                }
                // Value is a valid string. No checks required.
                "string" => {}
                _ => {
                    return Err(DqlError::parse(format!(
                        "Type {} not supported",
                        go_strconv::quote(typ)
                    )));
                }
            }
        }
    }
    Ok(())
}

/// The value of `f` when it names a query parameter (`$name`), `f` itself otherwise.
fn substitute_var(f: &str, vmap: &VarMap) -> Result<Option<String>, DqlError> {
    if !f.starts_with('$') {
        return Ok(None);
    }
    match vmap.get(f) {
        Some(va) if !va.typ.is_empty() => Ok(Some(va.value.clone())),
        _ => Err(DqlError::parse(format!("Variable not defined {f}"))),
    }
}

fn substitute_in_place(s: &mut String, vmap: &VarMap) -> Result<(), DqlError> {
    if let Some(v) = substitute_var(s, vmap)? {
        *s = v;
    }
    Ok(())
}

/// Substitutes the query parameters throughout a block.
pub(crate) fn substitute_variables(gq: &mut GraphQuery, vmap: &VarMap) -> Result<(), DqlError> {
    for val in gq.args.values_mut() {
        // v won't be empty as it's handled in parseDqlVariables.
        substitute_in_place(val, vmap)?;
    }
    if gq.uid.is_empty()
        && let Some(id_val) = gq.args.get("id")
    {
        let uids = parse_id(id_val)?;
        gq.uid.extend(uids);
        // Deleting it here because we don't need to fill it in query.go.
        gq.args.remove("id");
    }
    if let Some(m) = &mut gq.math_exp {
        subs(m, vmap)?;
    }
    if let Some(func) = &mut gq.func {
        substitute_in_place(&mut func.attr, vmap)?;
        for idx in 0..func.args.len() {
            if !func.args[idx].is_dql_var {
                continue;
            }
            substitute_in_place(&mut func.args[idx].value, vmap)?;
            if func.name == "regexp" {
                reg_exp_variable_filter(func, idx)?;
            }
        }
    }
    for child in &mut gq.children {
        substitute_variables(child, vmap)?;
    }
    if let Some(f) = &mut gq.filter {
        substitute_variables_filter(f, vmap)?;
    }
    if let Some(f) = &mut gq.facets_filter {
        substitute_variables_filter(f, vmap)?;
    }
    if let Some(var_map) = &gq.recurse_args.var_map {
        // Update the depth if the query gives the depth as a variable.
        if let Some(var_name) = var_map.get("depth") {
            let val = vmap
                .get(var_name)
                .ok_or_else(|| DqlError::parse(format!("variable {var_name} not defined")))?;
            gq.recurse_args.depth = go_strconv::parse_uint(&val.value, 0).map_err(|e| {
                DqlError::parse(format!("{var_name} should be type of integer: {e}"))
            })?;
        }
        // Update the loop if the query gives the loop as a variable.
        if let Some(var_name) = var_map.get("loop") {
            let val = vmap
                .get(var_name)
                .ok_or_else(|| DqlError::parse(format!("variable {var_name} not defined")))?;
            gq.recurse_args.allow_loop = go_strconv::parse_bool(&val.value).map_err(|e| {
                DqlError::parse(format!("{var_name} should be type of boolean: {e}"))
            })?;
        }
    }
    Ok(())
}

/// Splits a regular expression a query parameter gave into its expression and flags.
fn reg_exp_variable_filter(f: &mut Function, idx: usize) -> Result<(), DqlError> {
    let ra = parse_regex_args(&f.args[idx].value)?;
    // Regex functions take two args: the expression replaces the value, the flags follow.
    f.args[idx].value = ra.expr;
    f.args.push(Arg::new(ra.flags));
    Ok(())
}

fn substitute_variables_filter(f: &mut FilterTree, vmap: &VarMap) -> Result<(), DqlError> {
    if let Some(func) = &mut f.func {
        substitute_in_place(&mut func.attr, vmap)?;
        for idx in 0..func.args.len() {
            if !func.args[idx].is_dql_var {
                continue;
            }
            let name = func.args[idx].value.clone();
            if func.name == "uid" {
                // This is to support DQL variables in uid functions.
                let id_val = vmap.get(&name).ok_or_else(|| {
                    DqlError::parse(format!("Couldn't find value for DQL variable: [{name}]"))
                })?;
                let uids = parse_id(&id_val.value)?;
                func.uid.extend(uids);
                continue;
            }
            substitute_in_place(&mut func.args[idx].value, vmap)?;
            // We need to parse the regexp after substituting it from a DQL Variable.
            if func.name == "regexp" && vmap.contains_key(&name) {
                reg_exp_variable_filter(func, idx)?;
            }
        }
    }
    for child in &mut f.child {
        substitute_variables_filter(child, vmap)?;
    }
    Ok(())
}

/// Splits `/expr/flags`; the value may come from a query parameter that has not been checked yet.
pub(crate) fn parse_regex_args(val: &str) -> Result<RegexArgs, DqlError> {
    if val.is_empty() {
        return Err(DqlError::parse(format!("Found empty regex: [{val}]")));
    }
    let end = val.rfind('/').unwrap_or(0);
    if end < 1 || !val.starts_with('/') {
        return Err(DqlError::parse(format!("Found invalid regex: [{val}]")));
    }
    Ok(RegexArgs {
        expr: val[1..end].replace("\\/", "/"),
        flags: val[end + 1..].to_string(),
    })
}

/// Checks whether the query declares parameters, which a named query alone may, and parses its block.
pub(crate) fn get_variables_and_query(
    it: &mut It<'_, '_>,
    vmap: &mut VarMap,
) -> Result<Option<GraphQuery>, DqlError> {
    let mut name = String::new();
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_NAME => {
                if !name.is_empty() {
                    return Err(ierr(item, "Multiple word query name not allowed."));
                }
                name = item.val().to_string();
            }
            ITEM_LEFT_ROUND => {
                if name.is_empty() {
                    return Err(ierr(
                        item,
                        "Variables can be defined only in named queries.",
                    ));
                }
                parse_dql_variables(it, vmap)?;
                check_value_type(vmap)?;
            }
            ITEM_LEFT_CURL => return root::get_query(it).map(Some),
            _ => {}
        }
    }
    Ok(None)
}

/// The name of a query parameter after its `$`, with the `$`.
pub(crate) fn parse_var_name(it: &mut It<'_, '_>) -> Result<String, DqlError> {
    let mut val = String::from("$");
    let mut consume_at_least = false;
    loop {
        let items = it.peek(1)?;
        if items[0].typ() != ITEM_NAME {
            if !consume_at_least {
                return Err(iterr(it, "Expected variable name after $"));
            }
            break;
        }
        consume_at_least = true;
        val.push_str(items[0].val());
        // Consume the current item.
        if !it.next() {
            break;
        }
    }
    Ok(val)
}

/// Parses the declaration of the query parameters, `($a: int = 5, $b: string!)`.
fn parse_dql_variables(it: &mut It<'_, '_>, vmap: &mut VarMap) -> Result<(), DqlError> {
    let mut expect_arg = true;
    if it
        .peek_one()
        .is_some_and(|item| item.typ() == ITEM_RIGHT_ROUND)
    {
        return Ok(());
    }
    while it.next() {
        let var_name;
        // Get variable name.
        let mut item = it.item();
        match item.typ() {
            ITEM_DOLLAR => {
                if !expect_arg {
                    return Err(ierr(item, "Missing comma in var declaration"));
                }
                it.next();
                item = it.item();
                if item.typ() == ITEM_NAME {
                    var_name = format!("${}", item.val());
                } else {
                    return Err(ierr(
                        item,
                        format!("Expecting a variable name. Got: {item}"),
                    ));
                }
            }
            ITEM_RIGHT_ROUND => {
                if expect_arg {
                    return Err(ierr(item, "Invalid comma in var block"));
                }
                break;
            }
            ITEM_COMMA => {
                if expect_arg {
                    return Err(ierr(item, "Invalid comma in var block"));
                }
                expect_arg = true;
                continue;
            }
            _ => {
                return Err(ierr(
                    item,
                    format!(
                        "Unexpected item in place of variable. Got: {item} {}",
                        item.typ() == ITEM_DOLLAR
                    ),
                ));
            }
        }

        it.next();
        item = it.item();
        if item.typ() != ITEM_COLON {
            return Err(ierr(item, format!("Expecting a colon. Got: {item}")));
        }

        // Get variable type.
        it.next();
        item = it.item();
        if item.typ() != ITEM_NAME {
            return Err(ierr(
                item,
                format!("Expecting a variable type. Got: {item}"),
            ));
        }
        // Ensure that the type is not nil.
        let mut var_type = item.val().to_string();
        if var_type.is_empty() {
            return Err(ierr(item, "Type of a variable can't be empty"));
        }
        it.next();
        item = it.item();
        if item.typ() == ITEM_MATH_OP && item.val() == "!" {
            var_type.push('!');
            it.next();
            item = it.item();
        }
        // Insert the variable into the map. The variable might already be defined in the variable list passed
        // with the query.
        let value = vmap
            .get(&var_name)
            .map(|v| v.value.clone())
            .unwrap_or_default();
        vmap.insert(
            var_name.clone(),
            VarInfo {
                value,
                typ: var_type.clone(),
            },
        );

        // Check for '=' sign and optional default value.
        match item.typ() {
            ITEM_EQUAL => {
                it.next();
                let default = it.item();
                if default.typ() != ITEM_NAME {
                    return Err(ierr(
                        item,
                        format!("Expecting default value of a variable. Got: {item}"),
                    ));
                }
                if var_type.ends_with('!') {
                    return Err(ierr(
                        item,
                        format!("Type ending with ! can't have default value: Got: {var_type}"),
                    ));
                }
                // If value is empty replace, otherwise ignore the default value as the initialised value
                // overrides the default value.
                if vmap[&var_name].value.is_empty() {
                    let uq = unquote_if_quoted(default.val())?;
                    vmap.insert(
                        var_name,
                        VarInfo {
                            value: uq,
                            typ: var_type,
                        },
                    );
                }
            }
            ITEM_RIGHT_ROUND => break,
            _ => {
                // We consumed an extra item to see if it was an '=' sign, so move back.
                it.prev();
            }
        }
        expect_arg = false;
    }
    Ok(())
}

/// Unquotes `s` when it starts and ends with quotes, and returns it unchanged otherwise.
pub(crate) fn unquote_if_quoted(s: &str) -> Result<String, DqlError> {
    let b = s.as_bytes();
    if b.len() < 2 || b[0] != b'"' || b[b.len() - 1] != b'"' {
        return Ok(s.to_string());
    }
    go_strconv::unquote(s)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .ok_or_else(|| {
            DqlError::parse(format!(
                "could not unquote {}:: invalid syntax",
                go_strconv::quote(s)
            ))
        })
}

/// Parses an id list, `0x1` or `[0x1, 0x2]`, given as a query parameter.
pub(crate) fn parse_id(val: &str) -> Result<Vec<u64>, DqlError> {
    let val: String = val.chars().filter(|&c| c != ' ' && c != '\t').collect();
    if val.is_empty() {
        return Err(DqlError::parse("ID can't be empty"));
    }
    let num = |s: &str| go_strconv::parse_uint(s, 0).map_err(|e| DqlError::parse(e.to_string()));
    if !val.starts_with('[') {
        return Ok(vec![num(&val)?]);
    }
    if !val.ends_with(']') {
        return Err(DqlError::parse(format!(
            "Invalid id list at root. Got: {val}"
        )));
    }
    let mut uids = Vec::new();
    let mut buf = String::new();
    for c in val[1..].chars() {
        if c == ',' || c == ']' {
            if buf.is_empty() {
                continue;
            }
            uids.push(num(&buf)?);
            buf.clear();
            continue;
        }
        if c == '[' || c == ')' {
            return Err(DqlError::parse(format!(
                "Invalid id list at root. Got: {val}"
            )));
        }
        buf.push(c);
    }
    Ok(uids)
}

/// Parses `(a, b)` after `val` or `uid` into the variables the block needs; how many it named.
pub(crate) fn parse_var_list(
    it: &mut It<'_, '_>,
    needs_var: &mut Vec<VarContext>,
) -> Result<usize, DqlError> {
    let mut count = 0;
    let mut expect_arg = true;
    it.next();
    let item = it.item();
    if item.typ() != ITEM_LEFT_ROUND {
        return Err(ierr(item, "Expected a left round after var"));
    }
    while it.next() {
        let inner = it.item();
        match inner.typ() {
            ITEM_RIGHT_ROUND => break,
            ITEM_COMMA => {
                if expect_arg {
                    return Err(ierr(inner, "Expected a variable but got comma"));
                }
                expect_arg = true;
            }
            ITEM_NAME => {
                if !expect_arg {
                    return Err(ierr(
                        inner,
                        format!("Expected a variable but got {}", inner.val()),
                    ));
                }
                count += 1;
                needs_var.push(VarContext::new(inner.val(), VarType::Uid));
                expect_arg = false;
            }
            _ => {}
        }
    }
    if expect_arg {
        return Err(ierr(item, "Unnecessary comma in val()"));
    }
    Ok(count)
}
