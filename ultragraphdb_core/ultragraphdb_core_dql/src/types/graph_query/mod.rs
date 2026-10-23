/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use ultragraphdb_proto_pb::pb;

use crate::types::facet_order::FacetOrder;
use crate::types::filter_tree::FilterTree;
use crate::types::function::Function;
use crate::types::group_by_attr::GroupByAttr;
use crate::types::math_tree::MathTree;
use crate::types::recurse_args::RecurseArgs;
use crate::types::shortest_path_args::ShortestPathArgs;
use crate::types::var_context::VarContext;
use crate::types::vars::Vars;

/// A parsed query block in tree form, Dgraph's `GraphQuery`: the root of a block, or a predicate inside one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphQuery {
    pub(crate) uid: Vec<u64>,
    pub(crate) attr: String,
    pub(crate) langs: Vec<String>,
    pub(crate) alias: String,
    pub(crate) is_count: bool,
    pub(crate) is_internal: bool,
    pub(crate) is_groupby: bool,
    pub(crate) var: String,
    pub(crate) needs_var: Vec<VarContext>,
    pub(crate) func: Option<Function>,
    pub(crate) expand: String,
    pub(crate) args: BTreeMap<String, String>,
    pub(crate) order: Vec<pb::Order>,
    pub(crate) children: Vec<GraphQuery>,
    pub(crate) filter: Option<FilterTree>,
    pub(crate) math_exp: Option<MathTree>,
    pub(crate) normalize: bool,
    pub(crate) recurse: bool,
    pub(crate) recurse_args: RecurseArgs,
    pub(crate) shortest_path_args: ShortestPathArgs,
    pub(crate) cascade: Vec<String>,
    pub(crate) ignore_reflex: bool,
    pub(crate) facets: Option<pb::FacetParams>,
    pub(crate) facets_filter: Option<FilterTree>,
    pub(crate) groupby_attrs: Vec<GroupByAttr>,
    pub(crate) facet_var: Option<BTreeMap<String, String>>,
    pub(crate) facets_order: Vec<FacetOrder>,
    pub(crate) allowed_preds: Vec<String>,
    /// Nonempty for a fragment reference (spread), which expansion replaces.
    pub(crate) fragment: String,
    pub(crate) is_empty: bool,
}

impl GraphQuery {
    /// The uids the root names, `uid(0x1)` or `id: [0x1, 0x2]`.
    pub fn uid(&self) -> &[u64] {
        &self.uid
    }

    /// The predicate.
    pub fn attr(&self) -> &str {
        &self.attr
    }

    /// The language list, `name@en:ru`.
    pub fn langs(&self) -> &[String] {
        &self.langs
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// Whether the predicate is `count(pred)`.
    pub fn is_count(&self) -> bool {
        self.is_count
    }

    /// Whether the node is internal (`val()`, `math()`, an aggregation, `expand()`), not a stored predicate.
    pub fn is_internal(&self) -> bool {
        self.is_internal
    }

    /// Whether the node carries `@groupby`.
    pub fn is_groupby(&self) -> bool {
        self.is_groupby
    }

    /// The variable the node defines, `a as friends`.
    pub fn var(&self) -> &str {
        &self.var
    }

    /// The variables the node needs.
    pub fn needs_var(&self) -> &[VarContext] {
        &self.needs_var
    }

    /// The root function, `func: eq(name, "x")`.
    pub fn func(&self) -> Option<&Function> {
        self.func.as_ref()
    }

    /// What `expand()` expands: `_all_`, a variable, or a comma-separated type list.
    pub fn expand(&self) -> &str {
        &self.expand
    }

    /// The arguments: `first`, `offset`, `after`, `depth`, and the shortest path's `numpaths`, `minweight`, ...
    pub fn args(&self) -> &BTreeMap<String, String> {
        &self.args
    }

    /// The sort orders, in the order given.
    pub fn order(&self) -> &[pb::Order] {
        &self.order
    }

    pub fn children(&self) -> &[GraphQuery] {
        &self.children
    }

    pub fn filter(&self) -> Option<&FilterTree> {
        self.filter.as_ref()
    }

    /// The expression of `math(...)`.
    pub fn math_exp(&self) -> Option<&MathTree> {
        self.math_exp.as_ref()
    }

    pub fn normalize(&self) -> bool {
        self.normalize
    }

    pub fn recurse(&self) -> bool {
        self.recurse
    }

    pub fn recurse_args(&self) -> &RecurseArgs {
        &self.recurse_args
    }

    pub fn shortest_path_args(&self) -> &ShortestPathArgs {
        &self.shortest_path_args
    }

    /// The predicates `@cascade` requires; `__all__` for every child.
    pub fn cascade(&self) -> &[String] {
        &self.cascade
    }

    pub fn ignore_reflex(&self) -> bool {
        self.ignore_reflex
    }

    /// The facets `@facets(...)` asks for.
    pub fn facets(&self) -> Option<&pb::FacetParams> {
        self.facets.as_ref()
    }

    /// The filter `@facets(eq(...))` applies to facets.
    pub fn facets_filter(&self) -> Option<&FilterTree> {
        self.facets_filter.as_ref()
    }

    pub fn groupby_attrs(&self) -> &[GroupByAttr] {
        &self.groupby_attrs
    }

    /// The variables facets define, by facet; `None` without `@facets(...)`.
    pub fn facet_var(&self) -> Option<&BTreeMap<String, String>> {
        self.facet_var.as_ref()
    }

    pub fn facets_order(&self) -> &[FacetOrder] {
        &self.facets_order
    }

    /// The predicates an ACL-enabled query may read; the parser leaves them empty.
    pub fn allowed_preds(&self) -> &[String] {
        &self.allowed_preds
    }

    /// Whether the block has no starting function and hence no starting nodes: it aggregates variables defined
    /// in another block.
    pub fn is_empty(&self) -> bool {
        self.is_empty
    }

    pub(crate) fn is_fragment(&self) -> bool {
        !self.fragment.is_empty()
    }

    pub(crate) fn collect_vars(&self, v: &mut Vars) {
        if !self.var.is_empty() {
            v.defines.push(self.var.clone());
        }
        if let Some(fv) = &self.facet_var {
            v.defines.extend(fv.values().cloned());
        }
        v.needs
            .extend(self.needs_var.iter().map(|va| va.name.clone()));
        for ch in &self.children {
            ch.collect_vars(v);
        }
        if let Some(f) = &self.filter {
            f.collect_vars(v);
        }
        if let Some(m) = &self.math_exp {
            m.collect_vars(v);
        }
        for f in [&self.shortest_path_args.from, &self.shortest_path_args.to]
            .into_iter()
            .flatten()
        {
            if let Some(nv) = f.needs_var.first() {
                v.needs.push(nv.name.clone());
            }
        }
    }
}
