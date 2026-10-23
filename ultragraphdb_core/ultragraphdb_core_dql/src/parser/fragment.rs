/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Fragments: their definitions, and their expansion where they are spread.

use std::collections::HashMap;

use super::godeep::godeep;
use super::{It, ierr, iterr};
use crate::errors::dql_error::DqlError;
use crate::state::{ITEM_LEFT_CURL, ITEM_NAME};
use crate::types::fragment_node::FragmentNode;
use crate::types::graph_query::GraphQuery;

/// The children of a fragment, expanded first; a fragment entered and not yet exited is a cycle.
fn expand(
    name: &str,
    fmap: &mut HashMap<String, FragmentNode>,
) -> Result<Vec<GraphQuery>, DqlError> {
    let node = fmap
        .get_mut(name)
        .ok_or_else(|| DqlError::parse(format!("Missing fragment: {name}")))?;
    if !node.exited {
        if node.entered {
            return Err(DqlError::parse(format!("Cycle detected: {}", node.name)));
        }
        node.entered = true;
        let mut gq = std::mem::take(&mut node.gq);
        expand_fragments(&mut gq, fmap)?;
        let node = fmap
            .get_mut(name)
            .ok_or_else(|| DqlError::parse(format!("Missing fragment: {name}")))?;
        node.gq = gq;
        node.exited = true;
    }
    Ok(fmap[name].gq.children.clone())
}

/// Replaces each fragment spread among the block's children, at any depth, by the fragment's children, keeping
/// their order.
pub(crate) fn expand_fragments(
    gq: &mut GraphQuery,
    fmap: &mut HashMap<String, FragmentNode>,
) -> Result<(), DqlError> {
    let mut new_children = Vec::with_capacity(gq.children.len());
    for mut child in std::mem::take(&mut gq.children) {
        if child.is_fragment() {
            // Name of fragment being referenced.
            new_children.extend(expand(&child.fragment, fmap)?);
        } else {
            expand_fragments(&mut child, fmap)?;
            new_children.push(child);
        }
    }
    gq.children = new_children;
    Ok(())
}

/// Parses a fragment definition (not a reference).
pub(crate) fn get_fragment(it: &mut It<'_, '_>) -> Result<FragmentNode, DqlError> {
    let mut name = String::new();
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_NAME => {
                let v = item.val().trim();
                // The first nontrivial token is the fragment name; everything after it until a left curl is
                // ignored.
                if !v.is_empty() && name.is_empty() {
                    name = v.to_string();
                }
            }
            ITEM_LEFT_CURL => break,
            _ => {
                return Err(ierr(
                    item,
                    format!("Unexpected item in fragment: {} {}", item.typ(), item.val()),
                ));
            }
        }
    }
    if name.is_empty() {
        return Err(iterr(it, "Empty fragment name"));
    }
    let mut gq = GraphQuery::default();
    godeep(it, &mut gq)?;
    Ok(FragmentNode {
        name,
        gq,
        entered: false,
        exited: false,
    })
}
