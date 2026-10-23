/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `%v` and `%+v` of the protobuf messages Dgraph prints in its errors, their protobuf text: the fields set,
//! in field order. Go's protobuf text puts a random second space between fields, decided per build, which no port
//! can reproduce; this one puts one.

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_proto_pb::pb::{
    DirectedEdge, SchemaUpdate, directed_edge, posting, schema_update,
};

/// The protobuf text of an edge.
pub(crate) fn edge_text(e: &DirectedEdge) -> String {
    let mut fields = Vec::new();
    if e.entity != 0 {
        fields.push(format!("entity:{}", e.entity));
    }
    if !e.attr.is_empty() {
        fields.push(format!("attr:{}", go_strconv::quote(&e.attr)));
    }
    if !e.value.is_empty() {
        fields.push(format!("value:{}", go_strconv::quote(&e.value)));
    }
    if e.value_type != 0 {
        fields.push(format!("value_type:{}", val_type_name(e.value_type)));
    }
    if e.value_id != 0 {
        fields.push(format!("value_id:{}", e.value_id));
    }
    if !e.lang.is_empty() {
        fields.push(format!("lang:{}", go_strconv::quote(&e.lang)));
    }
    if e.op != 0 {
        let op = directed_edge::Op::try_from(e.op)
            .map_or_else(|_| e.op.to_string(), |o| o.as_str_name().to_string());
        fields.push(format!("op:{op}"));
    }
    if e.namespace != 0 {
        fields.push(format!("namespace:{}", e.namespace));
    }
    fields.join(" ")
}

/// The protobuf text of a schema update.
pub(crate) fn schema_text(s: &SchemaUpdate) -> String {
    let mut fields = Vec::new();
    if !s.predicate.is_empty() {
        fields.push(format!("predicate:{}", go_strconv::quote(&s.predicate)));
    }
    if s.value_type != 0 {
        fields.push(format!("value_type:{}", val_type_name(s.value_type)));
    }
    if s.directive != 0 {
        let d = schema_update::Directive::try_from(s.directive)
            .map_or_else(|_| s.directive.to_string(), |d| d.as_str_name().to_string());
        fields.push(format!("directive:{d}"));
    }
    for t in &s.tokenizer {
        fields.push(format!("tokenizer:{}", go_strconv::quote(t)));
    }
    for (set, name) in [
        (s.count, "count"),
        (s.list, "list"),
        (s.upsert, "upsert"),
        (s.lang, "lang"),
    ] {
        if set {
            fields.push(format!("{name}:true"));
        }
    }
    fields.join(" ")
}

/// The name of a value type as Go prints the protobuf enum, its number when it has none.
pub(crate) fn val_type_name(v: i32) -> String {
    posting::ValType::try_from(v).map_or_else(|_| v.to_string(), |t| t.as_str_name().to_string())
}
