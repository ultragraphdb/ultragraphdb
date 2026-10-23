/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! A block's children: predicates, aliases, variables, `count`, `val`, aggregations, `math`, `expand`,
//! `checkpwd`, fragment spreads, their arguments and directives.

use std::collections::HashSet;

use ultragraphdb_common_utils::{go_fmt, go_strconv};
use ultragraphdb_core_lex::ITEM_EOF;
use ultragraphdb_proto_pb::pb;

use super::directive::{parse_directive, parse_language_list, parse_type_list};
use super::function::parse_function;
use super::root::is_sortkey;
use super::variables::parse_var_list;
use super::{
    It, attr_and_lang, check_depth, collect_name, ierr, iterr, peek1, try_parse_item_type,
    try_skip_item_typ,
};
use crate::errors::dql_error::DqlError;
use crate::math::parse_math_func;
use crate::state::{
    ITEM_AT, ITEM_COLON, ITEM_COMMA, ITEM_DOLLAR, ITEM_LEFT_CURL, ITEM_LEFT_ROUND, ITEM_MATH_OP,
    ITEM_NAME, ITEM_PERIOD, ITEM_RIGHT_CURL, ITEM_RIGHT_ROUND,
};
use crate::types::arg::Arg;
use crate::types::count_type::CountType;
use crate::types::function::{Function, is_aggregator};
use crate::types::graph_query::GraphQuery;
use crate::types::pair::Pair;
use crate::types::var_type::VarType;

/// Whether the key is valid at non-root nodes.
pub(crate) fn valid_key(k: &str) -> bool {
    matches!(k, "orderasc" | "orderdesc" | "first" | "offset" | "after")
}

fn is_math_block(name: &str) -> bool {
    name == "math"
}

/// The node `godeep` attaches arguments, directives and children to: the block itself, its last child, or
/// none.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cur {
    Block,
    LastChild,
    Nil,
}

fn cur_mut(gq: &mut GraphQuery, cur: Cur) -> Option<&mut GraphQuery> {
    match cur {
        Cur::Block => Some(gq),
        Cur::LastChild => gq.children.last_mut(),
        Cur::Nil => None,
    }
}

/// Parses the arguments after a predicate, `(first: 10, orderasc: name@en)`.
fn parse_arguments(it: &mut It<'_, '_>, gq: &mut GraphQuery) -> Result<Vec<Pair>, DqlError> {
    let mut result = Vec::new();
    let mut expect_arg = true;
    let mut order_count = 0;
    while it.next() {
        let mut p = Pair::default();
        // Get key.
        let mut item = it.item();
        match item.typ() {
            ITEM_NAME => {
                if !expect_arg {
                    return Err(ierr(
                        item,
                        format!("Expecting a comma. But got: {}", item.val()),
                    ));
                }
                p.key = collect_name(it, item.val());
                if is_sortkey(&p.key) {
                    order_count += 1;
                }
                expect_arg = false;
            }
            ITEM_RIGHT_ROUND => {
                if expect_arg {
                    return Err(ierr(item, "Expected argument but got ')'."));
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
        }

        it.next();
        item = it.item();
        if item.typ() != ITEM_COLON {
            return Err(ierr(
                item,
                format!("Expecting a colon. Got: {item} in {}", gq.attr),
            ));
        }

        // Get value.
        it.next();
        item = it.item();
        let mut val = String::new();
        if item.val() == "val" {
            let count = parse_var_list(it, &mut gq.needs_var)?;
            if count != 1 {
                return Err(ierr(
                    item,
                    format!("Only one variable expected. Got {count}"),
                ));
            }
            if let Some(last) = gq.needs_var.last_mut() {
                last.typ = VarType::Value;
                p.val = last.name.clone();
            }
            let multiple = is_sortkey(&p.key) && order_count > 1;
            let pval = p.val.clone();
            result.push(p);
            if multiple {
                return Err(ierr(
                    item,
                    format!("Multiple sorting only allowed by predicates. Got: {pval}"),
                ));
            }
            continue;
        }

        if item.typ() == ITEM_DOLLAR {
            val = "$".to_string();
            it.next();
            item = it.item();
            if item.typ() != ITEM_NAME {
                return Err(ierr(item, format!("Expecting argument value. Got: {item}")));
            }
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
        } else if item.typ() != ITEM_NAME {
            return Err(ierr(item, format!("Expecting argument value. Got: {item}")));
        }

        p.val = collect_name(it, &format!("{val}{}", item.val()));

        // Get language list, if present
        if peek1(it).is_ok_and(|next| next.typ() == ITEM_AT) {
            it.next(); // consume '@'
            it.next(); // move forward
            let langs = parse_language_list(it)?;
            p.val = format!("{}@{}", p.val, langs.join(":"));
        }
        result.push(p);
    }
    Ok(result)
}

/// Checks that an empty block (one without a root function) asks only for aggregations and math.
fn validate_empty_block_item(it: &mut It<'_, '_>, val: &str) -> Result<(), DqlError> {
    let save_pos = it.save();
    let res = (|| {
        let mut fname = val.to_string();
        // Could have alias so peek forward to get actual function name.
        if try_skip_item_typ(it, ITEM_COLON) {
            let (item, ok) = try_parse_item_type(it, ITEM_NAME);
            if !ok {
                return Err(ierr(item, format!("Expected name. Got: {}", item.val())));
            }
            fname = item.val().to_string();
        }
        let ok = try_skip_item_typ(it, ITEM_LEFT_ROUND);
        if !ok || (!is_math_block(&fname) && !is_aggregator(&fname)) {
            return Err(iterr(
                it,
                format!(
                    "Only aggregation/math functions allowed inside empty blocks. Got: {fname}"
                ),
            ));
        }
        Ok(())
    })();
    it.restore(save_pos)?;
    res
}

/// A block whose children `godeep` is parsing, with the state Go's recursive `godeep` keeps in its locals.
struct Frame {
    gq: GraphQuery,
    /// The level of `gq`; its children sit one level below.
    level: usize,
    /// Whether `gq` is the enclosing frame's block itself, entered through a `{` directly inside it, rather than
    /// its last child.
    same_block: bool,
    count: CountType,
    alias: String,
    var_name: String,
    /// Tracks the current node, for nesting.
    curp: Cur,
}

impl Frame {
    fn new(gq: GraphQuery, level: usize, same_block: bool) -> Self {
        Self {
            gq,
            level,
            same_block,
            count: CountType::NotSeen,
            alias: String::new(),
            var_name: String::new(),
            curp: Cur::Block,
        }
    }
}

/// What the item a frame parsed asks of `godeep`.
enum Step {
    /// Parse the next item into the same frame.
    Next,
    /// Parse the children of a block from the next item on, in this new frame.
    Enter(Box<Frame>),
    /// The frame's block is complete.
    Exit,
}

/// Constructs the subgraph of a block at `level` from the lexed items, up to the block's closing brace.
///
/// Go's `godeep` recurses into each nested block; this keeps a stack of frames instead, so that the call stack
/// stays flat however deep the blocks nest, and refuses children past [`MAX_DEPTH`](super::MAX_DEPTH).
pub(crate) fn godeep(
    it: &mut It<'_, '_>,
    gq: &mut GraphQuery,
    level: usize,
) -> Result<(), DqlError> {
    let mut frames = vec![Frame::new(std::mem::take(gq), level, false)];
    while let Some(frame) = frames.last_mut() {
        let step = if it.next() {
            block_item(it, frame)?
        } else {
            Step::Exit
        };
        match step {
            Step::Next => {}
            Step::Enter(entered) => frames.push(*entered),
            Step::Exit => {
                let Some(done) = frames.pop() else { break };
                match frames.last_mut() {
                    Some(parent) if done.same_block => parent.gq = done.gq,
                    Some(parent) => parent.gq.children.push(done.gq),
                    None => *gq = done.gq,
                }
            }
        }
    }
    Ok(())
}

/// Parses the current item into the frame's block.
fn block_item(it: &mut It<'_, '_>, frame: &mut Frame) -> Result<Step, DqlError> {
    let Frame {
        gq,
        level,
        count,
        alias,
        var_name,
        curp,
        ..
    } = frame;
    let item = it.item();
    match item.typ() {
        ITEM_EOF | ITEM_RIGHT_CURL => return Ok(Step::Exit),
        ITEM_PERIOD => {
            // looking for ...
            let mut dots = 1;
            for _ in 0..2 {
                if it.next() && it.item().typ() == ITEM_PERIOD {
                    dots += 1;
                }
            }
            if dots != 3 {
                return Err(ierr(
                    item,
                    format!("Expected 3 periods (\"...\"), got {dots}."),
                ));
            }
            it.next();
            let item = it.item();
            if item.typ() == ITEM_NAME {
                // Unlike itemName, there is no nesting, so do not change "curp".
                let mut child = GraphQuery::default();
                child.fragment = item.val().to_string();
                gq.children.push(child);
            }
        }
        ITEM_NAME => {
            let Ok(peek) = it.peek(1) else {
                return Err(ierr(item, "Invalid query"));
            };
            if peek[0].typ() == ITEM_NAME && peek[0].val().to_lowercase() == "as" {
                *var_name = item.val().to_string();
                it.next(); // "As" was checked before.
                return Ok(Step::Next);
            }
            let mut val = collect_name(it, item.val());
            let val_lower = val.to_lowercase();

            if peek1(it)?.typ() == ITEM_COLON {
                if !alias.is_empty() {
                    return Err(ierr(item, "Invalid colon after alias declaration"));
                }
                *alias = val;
                it.next(); // Consume the itemcolon
                return Ok(Step::Next);
            }

            if gq.is_groupby && !is_aggregator(&val) && val != "count" && *count != CountType::Seen
            {
                // Only aggregator or count allowed inside the groupby block.
                return Err(iterr(
                    it,
                    format!("Only aggregator/count functions allowed inside @groupby. Got: {val}"),
                ));
            }

            if gq.is_empty {
                validate_empty_block_item(it, &val_lower)?;
            }

            if val_lower == "checkpwd" {
                let mut child = GraphQuery::default();
                child.var = std::mem::take(var_name);
                child.alias = std::mem::take(alias);
                it.prev();
                let mut func = parse_function(it, Some(gq), *level + 1)?;
                func.args.push(Arg::new(func.attr.clone()));
                child.attr = func.attr.clone();
                child.func = Some(func);
                gq.children.push(child);
                *curp = Cur::Nil;
                return Ok(Step::Next);
            }
            if is_aggregator(&val_lower) {
                let mut child = GraphQuery::default();
                child.attr = "val".to_string();
                child.var = std::mem::take(var_name);
                child.is_internal = true;
                child.alias = std::mem::take(alias);
                it.next();
                if it.item().typ() != ITEM_LEFT_ROUND {
                    it.prev();
                } else {
                    it.next();
                    if gq.is_groupby {
                        let item = it.item();
                        let attr = collect_name(it, item.val());
                        // Get language list, if present
                        if peek1(it).is_ok_and(|next| next.typ() == ITEM_AT) {
                            it.next(); // consume '@'
                            it.next(); // move forward
                            child.langs = parse_language_list(it)?;
                        }
                        child.attr = attr;
                        child.is_internal = false;
                    } else {
                        if it.item().val() != "val" {
                            return Err(iterr(
                                it,
                                format!(
                                    "Only variables allowed in aggregate functions. Got: {}",
                                    it.item().val()
                                ),
                            ));
                        }
                        let count = parse_var_list(it, &mut child.needs_var)?;
                        if count != 1 {
                            return Err(iterr(
                                it,
                                format!(
                                    "Expected one variable inside val() of aggregator but got {count}"
                                ),
                            ));
                        }
                        if let Some(last) = child.needs_var.last_mut() {
                            last.typ = VarType::Value;
                        }
                    }
                    child.func = Some(Function {
                        name: val_lower.clone(),
                        needs_var: child.needs_var.clone(),
                        ..Function::default()
                    });
                    it.next(); // Skip the closing ')'
                    gq.children.push(child);
                    *curp = Cur::Nil;
                    return Ok(Step::Next);
                }
            } else if is_math_block(&val_lower) {
                if var_name.is_empty() && alias.is_empty() {
                    return Err(iterr(
                        it,
                        "Function math should be used with a variable or have an alias",
                    ));
                }
                let (math_tree, again) = parse_math_func(it, *level + 1)?;
                if again {
                    return Err(iterr(
                        it,
                        "Comma encountered in math() at unexpected place.",
                    ));
                }
                let mut child = GraphQuery::default();
                child.attr = val.clone();
                child.alias = std::mem::take(alias);
                child.var = std::mem::take(var_name);
                child.math_exp = Some(math_tree);
                child.is_internal = true;
                gq.children.push(child);
                *curp = Cur::Nil;
                return Ok(Step::Next);
            } else if val_lower == "expand" {
                if !var_name.is_empty() {
                    return Err(iterr(
                        it,
                        format!("expand() cannot be used with a variable: {val}"),
                    ));
                }
                if !alias.is_empty() {
                    return Err(iterr(it, "expand() cannot have an alias"));
                }
                it.next(); // Consume the '('
                if it.item().typ() != ITEM_LEFT_ROUND {
                    return Err(iterr(it, "Invalid use of expand()"));
                }
                it.next();
                let item = it.item();
                let mut child = GraphQuery::default();
                child.attr = val.clone();
                child.is_internal = true;
                match item.val() {
                    "val" => {
                        let count = parse_var_list(it, &mut child.needs_var)?;
                        if count != 1 {
                            return Err(ierr(
                                item,
                                "Invalid use of expand(). Exactly one variable expected.",
                            ));
                        }
                        if let Some(last) = child.needs_var.last_mut() {
                            last.typ = VarType::List;
                            child.expand = last.name.clone();
                        }
                    }
                    "_all_" => child.expand = "_all_".to_string(),
                    "_forward_" => {
                        return Err(ierr(item, "Argument _forward_ has been deprecated"));
                    }
                    "_reverse_" => {
                        return Err(ierr(item, "Argument _reverse_ has been deprecated"));
                    }
                    _ => parse_type_list(it, &mut child)?,
                }
                it.next(); // Consume ')'
                gq.children.push(child);
                // Note: curp is not set to nil. So it can have children, filters, etc.
                *curp = Cur::LastChild;
                return Ok(Step::Next);
            } else if val_lower == "count" {
                if *count != CountType::NotSeen {
                    return Err(iterr(it, "Invalid mention of function count"));
                }
                *count = CountType::Seen;
                it.next();
                if it.item().typ() != ITEM_LEFT_ROUND {
                    it.prev();
                    *count = CountType::NotSeen;
                } else {
                    let peek = it.peek(2)?;
                    if peek[0].typ() == ITEM_RIGHT_ROUND {
                        return Err(iterr(it, "Cannot use count(), please use count(uid)"));
                    }
                    if peek[0].val() == "uid" && peek[1].typ() == ITEM_RIGHT_ROUND {
                        if gq.is_groupby {
                            // count(uid) case which occurs inside @groupby
                            val = "uid".to_string();
                            // Skip uid)
                            it.next();
                            it.next();
                        } else {
                            *count = CountType::NotSeen;
                            let mut child = GraphQuery::default();
                            child.attr = "uid".to_string();
                            child.alias = std::mem::take(alias);
                            child.var = std::mem::take(var_name);
                            child.is_count = true;
                            child.is_internal = true;
                            gq.children.push(child);
                            it.next();
                            it.next();
                            return Ok(Step::Next);
                        }
                    } else {
                        return Ok(Step::Next);
                    }
                }
            } else if val_lower == "val" {
                if !var_name.is_empty() {
                    return Err(iterr(it, "Cannot assign a variable to val()"));
                }
                if *count == CountType::Seen {
                    return Err(iterr(it, "Count of a variable is not allowed"));
                }
                if peek1(it)?.typ() == ITEM_LEFT_ROUND {
                    let mut child = GraphQuery::default();
                    child.attr = val.clone();
                    child.is_internal = true;
                    child.alias = std::mem::take(alias);
                    let count = parse_var_list(it, &mut child.needs_var)?;
                    if count != 1 {
                        return Err(iterr(
                            it,
                            "Invalid use of val(). Exactly one variable expected.",
                        ));
                    }
                    // Only value vars can be retrieved.
                    if let Some(last) = child.needs_var.last_mut() {
                        last.typ = VarType::Value;
                    }
                    gq.children.push(child);
                    *curp = Cur::Nil;
                    return Ok(Step::Next);
                }
            } else if val_lower == "uid" {
                if *count == CountType::Seen {
                    return Err(iterr(it, "Count of a variable is not allowed"));
                }
                if peek1(it)?.typ() == ITEM_LEFT_ROUND {
                    return Err(iterr(it, "Cannot do uid() of a variable"));
                }
            }
            // Fall: a predicate.
            {
                if *count == CountType::SeenWithPred {
                    return Err(iterr(
                        it,
                        "Multiple predicates not allowed in single count.",
                    ));
                }
                let mut child = GraphQuery::default();
                child.attr = val;
                child.is_count = *count == CountType::Seen;
                child.var = std::mem::take(var_name);
                child.alias = std::mem::take(alias);
                if gq.is_count {
                    return Err(iterr(
                        it,
                        "Cannot have children attributes when asking for count.",
                    ));
                }
                gq.children.push(child);
                *curp = Cur::LastChild;
                if *count == CountType::Seen {
                    *count = CountType::SeenWithPred;
                }
            }
        }
        ITEM_LEFT_CURL => {
            let Some(cur) = cur_mut(gq, *curp) else {
                return Err(iterr(it, "Query syntax invalid."));
            };
            if !cur.langs.is_empty() {
                return Err(iterr(
                    it,
                    format!(
                        "Cannot have children for attr: {} with lang tags: {}",
                        cur.attr,
                        go_fmt::string_slice(&cur.langs)
                    ),
                ));
            }
            // The children of the block entered sit two levels below this frame's block.
            check_depth(*level + 2)?;
            let entered = match *curp {
                // A `{` directly inside a block: Go's godeep recurses on the same block.
                Cur::Block => Frame::new(std::mem::take(gq), *level + 1, true),
                Cur::LastChild => match gq.children.pop() {
                    Some(child) => Frame::new(child, *level + 1, false),
                    None => return Err(iterr(it, "Query syntax invalid.")),
                },
                Cur::Nil => return Err(iterr(it, "Query syntax invalid.")),
            };
            return Ok(Step::Enter(Box::new(entered)));
        }
        ITEM_LEFT_ROUND => {
            let Some(cur) = cur_mut(gq, *curp) else {
                return Err(iterr(it, "Query syntax invalid."));
            };
            if cur.attr.is_empty() {
                return Err(iterr(it, "Predicate name cannot be empty."));
            }
            let args = parse_arguments(it, cur)?;
            // Stores args in GraphQuery, will be used later while retrieving results.
            let mut order: HashSet<String> = HashSet::new();
            for p in args {
                if !valid_key(&p.key) {
                    return Err(iterr(it, format!("Got invalid keyword: {}", p.key)));
                }
                if cur.args.contains_key(&p.key) {
                    return Err(iterr(
                        it,
                        format!(
                            "Got repeated key {} at level {}",
                            go_strconv::quote(&p.key),
                            go_strconv::quote(&cur.attr)
                        ),
                    ));
                }
                if p.val.is_empty() {
                    return Err(iterr(it, "Got empty argument"));
                }
                if is_sortkey(&p.key) {
                    if order.contains(&p.val) {
                        return Err(iterr(
                            it,
                            format!("Sorting by an attribute: [{}] can only be done once", p.val),
                        ));
                    }
                    let (attr, langs) = attr_and_lang(&p.val);
                    if langs.len() > 1 {
                        return Err(iterr(
                            it,
                            format!(
                                "Sorting by an attribute: [{}] can only be done on one language",
                                p.val
                            ),
                        ));
                    }
                    cur.order.push(pb::Order {
                        attr,
                        desc: p.key == "orderdesc",
                        langs,
                    });
                    order.insert(p.val);
                    continue;
                }
                cur.args.insert(p.key, p.val);
            }
        }
        ITEM_AT => {
            let cur_level = if *curp == Cur::Block {
                *level
            } else {
                *level + 1
            };
            parse_directive(it, cur_mut(gq, *curp), cur_level)?;
        }
        ITEM_RIGHT_ROUND => {
            if *count != CountType::SeenWithPred {
                return Err(iterr(it, "Invalid mention of brackets"));
            }
            *count = CountType::NotSeen;
        }
        _ => {}
    }
    Ok(Step::Next)
}
