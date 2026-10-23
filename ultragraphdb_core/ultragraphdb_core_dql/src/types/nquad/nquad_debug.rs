/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::value::Val;

use super::NQuad;

/// A bytes field in protobuf text: printable ASCII as is, everything else escaped.
fn bytes(b: &[u8]) -> String {
    let mut out = String::from("\"");
    for &c in b {
        match c {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(char::from(c)),
            _ => out.push_str(&format!("\\x{c:02x}")),
        }
    }
    out.push('"');
    out
}

fn value_text(v: &api::Value) -> String {
    match &v.val {
        None => String::new(),
        Some(Val::DefaultVal(s)) => format!("default_val:{}", go_strconv::quote(s)),
        Some(Val::BytesVal(b)) => format!("bytes_val:{}", bytes(b)),
        Some(Val::IntVal(i)) => format!("int_val:{i}"),
        Some(Val::BoolVal(b)) => format!("bool_val:{b}"),
        Some(Val::StrVal(s)) => format!("str_val:{}", go_strconv::quote(s)),
        Some(Val::DoubleVal(d)) => format!("double_val:{d}"),
        Some(Val::GeoVal(b)) => format!("geo_val:{}", bytes(b)),
        Some(Val::DateVal(b)) => format!("date_val:{}", bytes(b)),
        Some(Val::DatetimeVal(b)) => format!("datetime_val:{}", bytes(b)),
        Some(Val::PasswordVal(s)) => format!("password_val:{}", go_strconv::quote(s)),
        Some(Val::UidVal(u)) => format!("uid_val:{u}"),
        Some(Val::BigfloatVal(b)) => format!("bigfloat_val:{}", bytes(b)),
        Some(Val::Vfloat32Val(b)) => format!("vfloat32_val:{}", bytes(b)),
    }
}

fn facet_text(f: &api::Facet) -> String {
    let mut fields = Vec::new();
    if !f.key.is_empty() {
        fields.push(format!("key:{}", go_strconv::quote(&f.key)));
    }
    if !f.value.is_empty() {
        fields.push(format!("value:{}", bytes(&f.value)));
    }
    if f.val_type != 0 {
        let name = api::facet::ValType::try_from(f.val_type)
            .map_or_else(|_| f.val_type.to_string(), |t| t.as_str_name().to_string());
        fields.push(format!("val_type:{name}"));
    }
    for t in &f.tokens {
        fields.push(format!("tokens:{}", go_strconv::quote(t)));
    }
    if !f.alias.is_empty() {
        fields.push(format!("alias:{}", go_strconv::quote(&f.alias)));
    }
    fields.join(" ")
}

impl NQuad {
    /// Go's `%+v` of the N-Quad, its protobuf text `subject:"a" predicate:"b" ...`: Go's protobuf text puts a
    /// random second space between fields, decided per build, which no port can reproduce; this one puts one.
    pub(crate) fn go_debug(&self) -> String {
        let nq = &self.0;
        let mut fields = Vec::new();
        if !nq.subject.is_empty() {
            fields.push(format!("subject:{}", go_strconv::quote(&nq.subject)));
        }
        if !nq.predicate.is_empty() {
            fields.push(format!("predicate:{}", go_strconv::quote(&nq.predicate)));
        }
        if !nq.object_id.is_empty() {
            fields.push(format!("object_id:{}", go_strconv::quote(&nq.object_id)));
        }
        if let Some(v) = &nq.object_value {
            fields.push(format!("object_value:{{{}}}", value_text(v)));
        }
        if !nq.lang.is_empty() {
            fields.push(format!("lang:{}", go_strconv::quote(&nq.lang)));
        }
        for f in &nq.facets {
            fields.push(format!("facets:{{{}}}", facet_text(f)));
        }
        if nq.namespace != 0 {
            fields.push(format!("namespace:{}", nq.namespace));
        }
        fields.join(" ")
    }
}
