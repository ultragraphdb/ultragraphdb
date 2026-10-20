/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Directives on predicates (`@facets`, `@cascade`, `@normalize`, `@filter`, `@groupby`), language lists and
//! type lists.

use super::facets::parse_facets;
use super::filter::parse_filter;
use super::godeep::valid_key;
use super::{It, collect_name, ierr, iterr, peek1};
use crate::errors::dql_error::DqlError;
use crate::state::{
    ITEM_AT, ITEM_COLON, ITEM_COMMA, ITEM_LEFT_CURL, ITEM_LEFT_ROUND, ITEM_NAME, ITEM_PERIOD,
    ITEM_RIGHT_CURL, ITEM_RIGHT_ROUND, ITEM_STAR, STAR,
};
use crate::types::graph_query::GraphQuery;
use crate::types::group_by_attr::GroupByAttr;
use crate::utils::go_fmt::string_slice;

const ERR_EXPAND_TYPE: &str = "expand is only compatible with type filters";

/// Parses the cascade directive, `@cascade` or `@cascade(pred1, pred2, ...)`.
pub(crate) fn parse_cascade(it: &mut It<'_, '_>, gq: &mut GraphQuery) -> Result<(), DqlError> {
    let mut item = it.item();
    let Ok(next) = peek1(it) else {
        return Err(ierr(item, "Unable to peek lexer after cascade"));
    };
    // Without arguments: `@cascade {`, `@cascade }`, `@cascade @`, `@cascade\n someOtherPred`.
    if matches!(
        next.typ(),
        ITEM_LEFT_CURL | ITEM_RIGHT_CURL | ITEM_AT | ITEM_NAME
    ) {
        // __all__ implies @cascade i.e. implies values for all the children are mandatory.
        gq.cascade.push("__all__".to_string());
        return Ok(());
    }
    let mut count = 0;
    let mut expect_arg = true;
    it.next();
    item = it.item();
    if item.typ() != ITEM_LEFT_ROUND {
        return Err(ierr(
            item,
            format!("Expected a left round after cascade, got: {item}"),
        ));
    }
    while it.next() {
        let inner = it.item();
        match inner.typ() {
            ITEM_RIGHT_ROUND => break,
            ITEM_COMMA => {
                if expect_arg {
                    return Err(ierr(inner, "Expected a predicate but got comma"));
                }
                expect_arg = true;
            }
            ITEM_NAME => {
                if !expect_arg {
                    return Err(ierr(
                        inner,
                        format!("Expected a comma or right round but got: {}", inner.val()),
                    ));
                }
                let name = collect_name(it, inner.val());
                gq.cascade.push(name);
                count += 1;
                expect_arg = false;
            }
            _ => {
                return Err(ierr(
                    inner,
                    format!("Unexpected item while parsing: {}", inner.val()),
                ));
            }
        }
    }
    if expect_arg {
        // use the initial item to report error line and column numbers
        return Err(ierr(item, "Unnecessary comma in cascade()"));
    }
    if count == 0 {
        return Err(ierr(
            item,
            "At least one predicate required in parameterized cascade()",
        ));
    }
    Ok(())
}

/// Parses the groupby directive.
pub(crate) fn parse_groupby(it: &mut It<'_, '_>, gq: &mut GraphQuery) -> Result<(), DqlError> {
    let mut count = 0;
    let mut expect_arg = true;
    it.next();
    let item = it.item();
    let mut alias = String::new();
    if item.typ() != ITEM_LEFT_ROUND {
        return Err(ierr(item, "Expected a left round after groupby"));
    }
    while it.next() {
        let inner = it.item();
        match inner.typ() {
            ITEM_RIGHT_ROUND => break,
            ITEM_COMMA => {
                if expect_arg {
                    return Err(ierr(inner, "Expected a predicate but got comma"));
                }
                expect_arg = true;
            }
            ITEM_NAME => {
                if !expect_arg {
                    return Err(ierr(
                        inner,
                        format!("Expected a comma or right round but got: {}", inner.val()),
                    ));
                }
                let val = collect_name(it, inner.val());
                if peek1(it)?.typ() == ITEM_COLON {
                    if !alias.is_empty() {
                        return Err(ierr(inner, format!("Expected predicate after {alias}:")));
                    }
                    if valid_key(&val) {
                        return Err(ierr(
                            inner,
                            format!("Can't use keyword {val} as alias in groupby"),
                        ));
                    }
                    alias = val;
                    it.next(); // Consume the itemColon
                    continue;
                }
                let mut langs = Vec::new();
                if peek1(it).is_ok_and(|next| next.typ() == ITEM_AT) {
                    it.next(); // consume '@'
                    it.next(); // move forward
                    langs = parse_language_list(it)?;
                }
                gq.groupby_attrs.push(GroupByAttr {
                    attr: val,
                    alias: std::mem::take(&mut alias),
                    langs,
                });
                count += 1;
                expect_arg = false;
            }
            _ => {}
        }
    }
    if expect_arg {
        // use the initial item to report error line and column numbers
        return Err(ierr(item, "Unnecessary comma in groupby()"));
    }
    if count == 0 {
        return Err(ierr(item, "Expected atleast one attribute in groupby"));
    }
    Ok(())
}

/// Parses `(Type1, Type2)` of `expand()` into a comma-separated list, the first type being the current item.
pub(crate) fn parse_type_list(it: &mut It<'_, '_>, gq: &mut GraphQuery) -> Result<(), DqlError> {
    let mut type_list = it.item().val().to_string();
    let mut expect_arg = false;
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_RIGHT_ROUND => {
                it.prev();
                break;
            }
            ITEM_COMMA => {
                if expect_arg {
                    return Err(ierr(item, "Expected a variable but got comma"));
                }
                expect_arg = true;
            }
            ITEM_NAME => {
                if !expect_arg {
                    return Err(ierr(
                        item,
                        format!("Expected a variable but got {}", item.val()),
                    ));
                }
                type_list = format!("{type_list},{}", item.val());
                expect_arg = false;
            }
            _ => {
                return Err(ierr(
                    item,
                    format!("Unexpected token {} when reading a type list", item.val()),
                ));
            }
        }
    }
    if expect_arg {
        return Err(ierr(it.item(), "Unnecessary comma in val()"));
    }
    gq.expand = type_list;
    Ok(())
}

/// Parses the directive or language list after an `@`, the current item, onto `curp`; `None` where no
/// directive is allowed.
pub(crate) fn parse_directive(
    it: &mut It<'_, '_>,
    curp: Option<&mut GraphQuery>,
) -> Result<(), DqlError> {
    let mut valid = true;
    it.prev();
    let item = it.item();
    if item.typ() == ITEM_LEFT_CURL {
        // Ideally we should check that curp was created at current depth.
        valid = false;
    }
    it.next();

    let is_expand = curp.as_ref().is_some_and(|c| !c.expand.is_empty());
    // No directive is allowed on pb.subgraph like expand all (except type filters), value variables, etc.
    let curp = match curp {
        Some(c) if valid && !(c.is_internal && !is_expand) => c,
        _ => return Err(ierr(item, "Invalid use of directive.")),
    };

    it.next();
    let item = it.item();
    let peek = it.peek(1);
    let peek = match peek {
        Ok(p) if item.typ() == ITEM_NAME => p,
        _ => return Err(ierr(item, "Expected directive or language list")),
    };
    if is_expand && item.val() != "filter" {
        return Err(ierr(item, ERR_EXPAND_TYPE));
    }

    match item.val() {
        // because @facets can come w/t '()'
        "facets" => {
            let res = parse_facets(it)?;
            if let Some(f) = res.f {
                curp.facet_var = res.vmap;
                curp.facets_order = res.facets_order;
                if curp.facets.is_some() {
                    return Err(ierr(item, "Only one facets allowed"));
                }
                curp.facets = Some(f);
            } else if let Some(ft) = res.ft {
                if curp.facets_filter.is_some() {
                    return Err(ierr(item, "Only one facets filter allowed"));
                }
                if ft.has_vars() {
                    return Err(ierr(item, "variables are not allowed in facets filter."));
                }
                curp.facets_filter = Some(ft);
            } else {
                return Err(ierr(item, "Facets parsing failed."));
            }
        }
        "cascade" => parse_cascade(it, curp)?,
        "normalize" => curp.normalize = true,
        // this is a directive
        name if peek[0].typ() == ITEM_LEFT_ROUND => match name {
            "filter" => {
                if curp.filter.is_some() {
                    return Err(ierr(
                        item,
                        "Use AND, OR and round brackets instead of multiple filter directives.",
                    ));
                }
                let filter = parse_filter(it)?;
                if is_expand
                    && filter
                        .as_ref()
                        .and_then(|f| f.func.as_ref())
                        .is_some_and(|f| f.name != "type")
                {
                    return Err(ierr(item, ERR_EXPAND_TYPE));
                }
                curp.filter = filter;
            }
            "groupby" => {
                if curp.is_groupby {
                    return Err(ierr(item, "Only one group by directive allowed."));
                }
                curp.is_groupby = true;
                parse_groupby(it, curp)?;
            }
            _ => return Err(ierr(item, format!("Unknown directive [{name}]"))),
        },
        // this is a language list
        _ if !curp.attr.is_empty() && curp.langs.is_empty() => {
            curp.langs = parse_language_list(it)?;
            if curp.langs.is_empty() {
                return Err(ierr(
                    item,
                    format!("Expected at least 1 language in list for {}", curp.attr),
                ));
            }
        }
        name => {
            return Err(ierr(
                item,
                format!("Expected directive or language list, got @{name}"),
            ));
        }
    }
    Ok(())
}

/// Parses a language list, `en:ru:.`, from the current item, leaving the iterator on its last language.
pub(crate) fn parse_language_list(it: &mut It<'_, '_>) -> Result<Vec<String>, DqlError> {
    let mut item = it.item();
    let mut langs = Vec::new();
    while matches!(item.typ(), ITEM_NAME | ITEM_PERIOD | ITEM_STAR) {
        langs.push(item.val().to_string());
        it.next();
        if it.item().typ() == ITEM_COLON {
            it.next();
        } else {
            break;
        }
        item = it.item();
    }
    if it.item().typ() == ITEM_PERIOD && peek1(it)?.typ() == ITEM_PERIOD {
        return Err(iterr(
            it,
            "Expected only one dot(.) while parsing language list.",
        ));
    }
    it.prev();
    if langs.len() > 1 && langs.iter().any(|lang| lang == STAR.to_string().as_str()) {
        return Err(DqlError::parse(format!(
            "If * is used, no other languages are allowed in the language list. Found {}",
            string_slice(&langs)
        )));
    }
    Ok(langs)
}
