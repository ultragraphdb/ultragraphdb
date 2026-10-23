/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::GraphQuery;

impl PartialEq for GraphQuery {
    /// Compares the trees block by block from a stack rather than by recursion, so that comparing deep trees takes
    /// no more call stack than comparing one block (ledger 32).
    fn eq(&self, other: &Self) -> bool {
        let mut todo = vec![(self, other)];
        while let Some((a, b)) = todo.pop() {
            if !a.eq_block(b) {
                return false;
            }
            todo.extend(a.children.iter().zip(&b.children));
        }
        true
    }
}

impl GraphQuery {
    /// Whether the blocks are equal, their children aside but for their number. Every field is named, so that a
    /// field added to the type cannot be left out of the comparison.
    fn eq_block(&self, other: &Self) -> bool {
        let GraphQuery {
            uid: uid_a,
            attr: attr_a,
            langs: langs_a,
            alias: alias_a,
            is_count: is_count_a,
            is_internal: is_internal_a,
            is_groupby: is_groupby_a,
            var: var_a,
            needs_var: needs_var_a,
            func: func_a,
            expand: expand_a,
            args: args_a,
            order: order_a,
            children: children_a,
            filter: filter_a,
            math_exp: math_exp_a,
            normalize: normalize_a,
            recurse: recurse_a,
            recurse_args: recurse_args_a,
            shortest_path_args: shortest_path_args_a,
            cascade: cascade_a,
            ignore_reflex: ignore_reflex_a,
            facets: facets_a,
            facets_filter: facets_filter_a,
            groupby_attrs: groupby_attrs_a,
            facet_var: facet_var_a,
            facets_order: facets_order_a,
            allowed_preds: allowed_preds_a,
            fragment: fragment_a,
            is_empty: is_empty_a,
        } = self;
        let GraphQuery {
            uid: uid_b,
            attr: attr_b,
            langs: langs_b,
            alias: alias_b,
            is_count: is_count_b,
            is_internal: is_internal_b,
            is_groupby: is_groupby_b,
            var: var_b,
            needs_var: needs_var_b,
            func: func_b,
            expand: expand_b,
            args: args_b,
            order: order_b,
            children: children_b,
            filter: filter_b,
            math_exp: math_exp_b,
            normalize: normalize_b,
            recurse: recurse_b,
            recurse_args: recurse_args_b,
            shortest_path_args: shortest_path_args_b,
            cascade: cascade_b,
            ignore_reflex: ignore_reflex_b,
            facets: facets_b,
            facets_filter: facets_filter_b,
            groupby_attrs: groupby_attrs_b,
            facet_var: facet_var_b,
            facets_order: facets_order_b,
            allowed_preds: allowed_preds_b,
            fragment: fragment_b,
            is_empty: is_empty_b,
        } = other;
        uid_a == uid_b
            && attr_a == attr_b
            && langs_a == langs_b
            && alias_a == alias_b
            && is_count_a == is_count_b
            && is_internal_a == is_internal_b
            && is_groupby_a == is_groupby_b
            && var_a == var_b
            && needs_var_a == needs_var_b
            && func_a == func_b
            && expand_a == expand_b
            && args_a == args_b
            && order_a == order_b
            && children_a.len() == children_b.len()
            && filter_a == filter_b
            && math_exp_a == math_exp_b
            && normalize_a == normalize_b
            && recurse_a == recurse_b
            && recurse_args_a == recurse_args_b
            && shortest_path_args_a == shortest_path_args_b
            && cascade_a == cascade_b
            && ignore_reflex_a == ignore_reflex_b
            && facets_a == facets_b
            && facets_filter_a == facets_filter_b
            && groupby_attrs_a == groupby_attrs_b
            && facet_var_a == facet_var_b
            && facets_order_a == facets_order_b
            && allowed_preds_a == allowed_preds_b
            && fragment_a == fragment_b
            && is_empty_a == is_empty_b
    }
}
