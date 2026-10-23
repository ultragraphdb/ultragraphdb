/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::GraphQuery;
use crate::utils::tree_debug::{Field, value, write_tree};

impl fmt::Debug for GraphQuery {
    /// The layout `#[derive(Debug)]` gives, written from a stack rather than by recursion, so that formatting a
    /// deep tree takes no more call stack than formatting one block (ledger 32).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_tree(f, self, "GraphQuery", |node, pretty, level| {
            // Every field is named, so that a field added to the type cannot be left out.
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
            } = node;
            vec![
                Field::Value("uid", value(uid, pretty, level)),
                Field::Value("attr", value(attr, pretty, level)),
                Field::Value("langs", value(langs, pretty, level)),
                Field::Value("alias", value(alias, pretty, level)),
                Field::Value("is_count", value(is_count, pretty, level)),
                Field::Value("is_internal", value(is_internal, pretty, level)),
                Field::Value("is_groupby", value(is_groupby, pretty, level)),
                Field::Value("var", value(var, pretty, level)),
                Field::Value("needs_var", value(needs_var, pretty, level)),
                Field::Value("func", value(func, pretty, level)),
                Field::Value("expand", value(expand, pretty, level)),
                Field::Value("args", value(args, pretty, level)),
                Field::Value("order", value(order, pretty, level)),
                Field::Children("children", children),
                Field::Value("filter", value(filter, pretty, level)),
                Field::Value("math_exp", value(math_exp, pretty, level)),
                Field::Value("normalize", value(normalize, pretty, level)),
                Field::Value("recurse", value(recurse, pretty, level)),
                Field::Value("recurse_args", value(recurse_args, pretty, level)),
                Field::Value(
                    "shortest_path_args",
                    value(shortest_path_args, pretty, level),
                ),
                Field::Value("cascade", value(cascade, pretty, level)),
                Field::Value("ignore_reflex", value(ignore_reflex, pretty, level)),
                Field::Value("facets", value(facets, pretty, level)),
                Field::Value("facets_filter", value(facets_filter, pretty, level)),
                Field::Value("groupby_attrs", value(groupby_attrs, pretty, level)),
                Field::Value("facet_var", value(facet_var, pretty, level)),
                Field::Value("facets_order", value(facets_order, pretty, level)),
                Field::Value("allowed_preds", value(allowed_preds, pretty, level)),
                Field::Value("fragment", value(fragment, pretty, level)),
                Field::Value("is_empty", value(is_empty, pretty, level)),
            ]
        })
    }
}
