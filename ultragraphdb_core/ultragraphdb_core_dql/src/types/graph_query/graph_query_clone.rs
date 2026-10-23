/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::GraphQuery;

impl Clone for GraphQuery {
    /// Copies the tree block by block along a path kept on the heap rather than by recursion, so that cloning a
    /// deep tree takes no more call stack than cloning one block (ledger 32).
    fn clone(&self) -> Self {
        // The blocks above the one being copied, each with its copy so far.
        let mut path: Vec<(&GraphQuery, GraphQuery)> = Vec::new();
        let mut src = self;
        let mut copy = src.clone_block();
        loop {
            if let Some(child) = src.children.get(copy.children.len()) {
                let child_copy = child.clone_block();
                path.push((src, std::mem::replace(&mut copy, child_copy)));
                src = child;
                continue;
            }
            match path.pop() {
                Some((parent, mut parent_copy)) => {
                    parent_copy.children.push(copy);
                    src = parent;
                    copy = parent_copy;
                }
                None => return copy,
            }
        }
    }
}

impl GraphQuery {
    /// A copy of the block without its children. Every field is named, so that a field added to the type cannot
    /// be left out of the copy.
    fn clone_block(&self) -> Self {
        let GraphQuery {
            uid,
            attr,
            langs,
            alias,
            is_count,
            is_internal,
            is_groupby,
            var,
            needs_var,
            func,
            expand,
            args,
            order,
            children,
            filter,
            math_exp,
            normalize,
            recurse,
            recurse_args,
            shortest_path_args,
            cascade,
            ignore_reflex,
            facets,
            facets_filter,
            groupby_attrs,
            facet_var,
            facets_order,
            allowed_preds,
            fragment,
            is_empty,
        } = self;
        GraphQuery {
            uid: uid.clone(),
            attr: attr.clone(),
            langs: langs.clone(),
            alias: alias.clone(),
            is_count: *is_count,
            is_internal: *is_internal,
            is_groupby: *is_groupby,
            var: var.clone(),
            needs_var: needs_var.clone(),
            func: func.clone(),
            expand: expand.clone(),
            args: args.clone(),
            order: order.clone(),
            children: Vec::with_capacity(children.len()),
            filter: filter.clone(),
            math_exp: math_exp.clone(),
            normalize: *normalize,
            recurse: *recurse,
            recurse_args: recurse_args.clone(),
            shortest_path_args: shortest_path_args.clone(),
            cascade: cascade.clone(),
            ignore_reflex: *ignore_reflex,
            facets: facets.clone(),
            facets_filter: facets_filter.clone(),
            groupby_attrs: groupby_attrs.clone(),
            facet_var: facet_var.clone(),
            facets_order: facets_order.clone(),
            allowed_preds: allowed_preds.clone(),
            fragment: fragment.clone(),
            is_empty: *is_empty,
        }
    }
}
