/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The `@facets` directive: a facet list with aliases, variables and orders, or a facets filter.

use std::collections::{BTreeMap, HashSet};

use ultragraphdb_proto_pb::pb;

use super::filter::parse_filter;
use super::{It, collect_name, ierr, iterr, try_parse_item_type, try_skip_item_val};
use crate::errors::dql_error::DqlError;
use crate::state::{ITEM_COLON, ITEM_COMMA, ITEM_LEFT_ROUND, ITEM_NAME, ITEM_RIGHT_ROUND};
use crate::types::facet_item::FacetItem;
use crate::types::facet_order::FacetOrder;
use crate::types::facet_res::FacetRes;

pub(crate) fn parse_facets(it: &mut It<'_, '_>) -> Result<FacetRes, DqlError> {
    if let Some(res) = try_parse_facet_list(it)? {
        return Ok(res);
    }
    Ok(FacetRes {
        ft: parse_filter(it)?,
        ..FacetRes::default()
    })
}

/// Parses `[{orderdesc|orderasc|alias}:] [varname as] name`. `Ok(None)` when the items are no facet item: the
/// iterator is back where it was, and another parse may succeed.
fn try_parse_facet_item(it: &mut It<'_, '_>) -> Result<Option<FacetItem>, DqlError> {
    let save_pos = it.save();
    let mut res = FacetItem::default();
    let (mut item, ok) = try_parse_item_type(it, ITEM_NAME);
    if !ok {
        it.restore(save_pos);
        return Ok(None);
    }
    let is_orderasc = item.val() == "orderasc";
    if try_parse_item_type(it, ITEM_COLON).1 {
        if is_orderasc || item.val() == "orderdesc" {
            res.ordered = true;
            res.orderdesc = !is_orderasc;
        } else {
            res.alias = item.val().to_string();
        }
        // Step past colon.
        let (next, ok) = try_parse_item_type(it, ITEM_NAME);
        if !ok {
            return Err(ierr(next, "Expected name after colon"));
        }
        item = next;
    }
    // We've possibly set ordered, orderdesc, alias and now we have consumed another item, which is a name.
    let name1 = item.val();
    // Now try to consume "as".
    if !try_skip_item_val(it, "as") {
        res.name = collect_name(it, name1);
        return Ok(Some(res));
    }
    let (item, ok) = try_parse_item_type(it, ITEM_NAME);
    if !ok {
        return Err(ierr(item, "Expected name in facet list"));
    }
    res.name = collect_name(it, item.val());
    res.var_name = name1.to_string();
    Ok(Some(res))
}

/// Parses a facet list. `Ok(None)` when the items are no facet list: the iterator is back where it was, and
/// [`parse_filter`] may succeed.
fn try_parse_facet_list(it: &mut It<'_, '_>) -> Result<Option<FacetRes>, DqlError> {
    let save_pos = it.save();
    let no_parse = |it: &mut It<'_, '_>| {
        it.restore(save_pos);
        Ok(None)
    };

    // Skip past '('
    if !try_parse_item_type(it, ITEM_LEFT_ROUND).1 {
        it.restore(save_pos);
        return Ok(Some(FacetRes {
            f: Some(pb::FacetParams {
                all_keys: true,
                param: Vec::new(),
            }),
            vmap: Some(BTreeMap::new()),
            ..FacetRes::default()
        }));
    }

    let mut facet_var: BTreeMap<String, String> = BTreeMap::new();
    let mut facets = pb::FacetParams::default();
    let mut facets_order: Vec<FacetOrder> = Vec::new();

    if try_parse_item_type(it, ITEM_RIGHT_ROUND).1 {
        // @facets() just parses to an empty set of facets.
        return Ok(Some(FacetRes {
            f: Some(facets),
            vmap: Some(facet_var),
            facets_order,
            ft: None,
        }));
    }

    let mut facets_order_keys: HashSet<String> = HashSet::new();
    loop {
        // We've just consumed a leftRound or a comma. Parse a facet item.
        let Some(facet_item) = try_parse_facet_item(it)? else {
            return no_parse(it);
        };
        // Combine the facet item with our result.
        if !facet_item.var_name.is_empty() {
            if facet_var.contains_key(&facet_item.name) {
                return Err(iterr(
                    it,
                    format!("Duplicate variable mappings for facet {}", facet_item.name),
                ));
            }
            facet_var.insert(facet_item.name.clone(), facet_item.var_name.clone());
        }
        facets.param.push(pb::FacetParam {
            key: facet_item.name.clone(),
            alias: facet_item.alias.clone(),
        });
        if facet_item.ordered {
            if !facets_order_keys.insert(facet_item.name.clone()) {
                return Err(iterr(
                    it,
                    format!(
                        "Sorting by facet: [{}] can only be done once",
                        facet_item.name
                    ),
                ));
            }
            facets_order.push(FacetOrder {
                key: facet_item.name.clone(),
                desc: facet_item.orderdesc,
            });
        }

        // Now what? Either close-paren or a comma.
        if try_parse_item_type(it, ITEM_RIGHT_ROUND).1 {
            facets.param.sort_by(|a, b| a.key.cmp(&b.key));
            // Deduplicate facets, keeping the last of each key.
            let mut out: Vec<pb::FacetParam> = Vec::with_capacity(facets.param.len());
            for p in facets.param {
                match out.last_mut() {
                    Some(last) if last.key == p.key => *last = p,
                    _ => out.push(p),
                }
            }
            facets.param = out;
            return Ok(Some(FacetRes {
                f: Some(facets),
                vmap: Some(facet_var),
                facets_order,
                ft: None,
            }));
        }
        let (item, ok) = try_parse_item_type(it, ITEM_COMMA);
        if !ok {
            if facets.param.len() < 2 {
                // We have only consumed `'@facets' '(' <facetItem>`, which means parseFilter might succeed.
                return no_parse(it);
            }
            // We've consumed `'@facets' '(' <facetItem> ',' <facetItem>`, so this is definitely not a filter.
            return Err(ierr(
                item,
                format!("Expected ',' or ')' in facet list: {}", item.val()),
            ));
        }
    }
}
