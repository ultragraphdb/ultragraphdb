/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The JSON form of a parsed query that tools/golden/dql.go writes: slices never null, absent nodes null, math
//! constants as their type and text.

use serde_json::{Value as Json, json};
use ultragraphdb_core_dql::{FilterTree, Function, GraphQuery, MathTree, ParseResult, VarContext};
use ultragraphdb_core_types::{TypeId, Value};
use ultragraphdb_proto_dgraph::api;

fn needs_var(vs: &[VarContext]) -> Json {
    vs.iter()
        .map(|v| json!({"name": v.name(), "typ": v.typ().value()}))
        .collect()
}

fn function(f: Option<&Function>) -> Json {
    let Some(f) = f else { return Json::Null };
    let args: Json = f
        .args()
        .iter()
        .map(|a| json!({"value": a.value(), "is_value_var": a.is_value_var(), "is_dql_var": a.is_dql_var()}))
        .collect();
    json!({
        "attr": f.attr(), "lang": f.lang(), "name": f.name(), "args": args, "uid": f.uid(),
        "needs_var": needs_var(f.needs_var()), "is_count": f.is_count(), "is_value_var": f.is_value_var(),
        "is_len_var": f.is_len_var(),
    })
}

fn filter(f: Option<&FilterTree>) -> Json {
    let Some(f) = f else { return Json::Null };
    let child: Json = f.child().iter().map(|c| filter(Some(c))).collect();
    json!({"op": f.op(), "child": child, "func": function(f.func())})
}

/// Go's `strconv.FormatFloat(f, 'E', -1, 64)`.
pub(crate) fn format_e(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf" } else { "-Inf" }.into();
    }
    let s = format!("{f:E}");
    let (m, e) = s.split_once('E').unwrap();
    let (sign, d) = e.strip_prefix('-').map_or(('+', e), |d| ('-', d));
    format!("{m}E{sign}{d:0>2}")
}

fn math(t: Option<&MathTree>) -> Json {
    let Some(t) = t else { return Json::Null };
    let c = t.const_val();
    let value = match c.value() {
        None => Json::Null,
        Some(Value::Int(i)) => json!(i.to_string()),
        Some(Value::Float(f)) => json!(format_e(*f)),
        Some(Value::Str(s)) => json!(String::from_utf8_lossy(s)),
        Some(Value::Bool(b)) => json!(b.to_string()),
        Some(v) => json!(format!("{v:?}")),
    };
    let child: Json = t.child().iter().map(|c| math(Some(c))).collect();
    json!({"fn": t.fn_name(), "var": t.var(), "const": {"tid": TypeId::enum_value(c.tid()), "value": value}, "child": child})
}

pub(crate) fn graph_query(gq: &GraphQuery) -> Json {
    let order: Json = gq
        .order()
        .iter()
        .map(|o| json!({"attr": o.attr, "desc": o.desc, "langs": o.langs}))
        .collect();
    let children: Json = gq.children().iter().map(graph_query).collect();
    let groupby: Json = gq
        .groupby_attrs()
        .iter()
        .map(|g| json!({"attr": g.attr(), "alias": g.alias(), "langs": g.langs()}))
        .collect();
    let facets = gq.facets().map_or(Json::Null, |f| {
        let param: Json = f
            .param
            .iter()
            .map(|p| json!({"key": p.key, "alias": p.alias}))
            .collect();
        json!({"all_keys": f.all_keys, "param": param})
    });
    let facets_order: Json = gq
        .facets_order()
        .iter()
        .map(|f| json!({"key": f.key(), "desc": f.desc()}))
        .collect();
    let ra = gq.recurse_args();
    json!({
        "uid": gq.uid(), "attr": gq.attr(), "langs": gq.langs(), "alias": gq.alias(),
        "is_count": gq.is_count(), "is_internal": gq.is_internal(), "is_groupby": gq.is_groupby(), "var": gq.var(),
        "needs_var": needs_var(gq.needs_var()), "func": function(gq.func()), "expand": gq.expand(),
        "args": gq.args(), "order": order, "children": children, "filter": filter(gq.filter()),
        "math_exp": math(gq.math_exp()), "normalize": gq.normalize(), "recurse": gq.recurse(),
        "recurse_args": {"depth": ra.depth(), "allow_loop": ra.allow_loop(), "var_map": ra.var_map()},
        "shortest_path_args": {
            "from": function(gq.shortest_path_args().from()), "to": function(gq.shortest_path_args().to()),
        },
        "cascade": gq.cascade(), "ignore_reflex": gq.ignore_reflex(), "facets": facets,
        "facets_filter": filter(gq.facets_filter()), "groupby_attrs": groupby, "facet_var": gq.facet_var(),
        "facets_order": facets_order, "allowed_preds": gq.allowed_preds(), "is_empty": gq.is_empty(),
    })
}

pub(crate) fn result(res: &ParseResult) -> Json {
    let query: Json = res.query().iter().map(graph_query).collect();
    let vars: Json = res
        .query_vars()
        .iter()
        .map(|v| json!({"defines": v.defines(), "needs": v.needs()}))
        .collect();
    let schema = res.schema().map_or(
        Json::Null,
        |s| json!({"predicates": s.predicates, "fields": s.fields, "types": s.types}),
    );
    json!({"query": query, "query_vars": vars, "schema": schema})
}

pub(crate) fn request(req: &api::Request) -> Json {
    let mutations: Json = req
        .mutations
        .iter()
        .map(|m| {
            json!({
                "set_nquads": String::from_utf8_lossy(&m.set_nquads), "del_nquads": String::from_utf8_lossy(&m.del_nquads),
                "cond": m.cond,
            })
        })
        .collect();
    json!({"query": req.query, "mutations": mutations})
}
