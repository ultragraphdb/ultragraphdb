/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! N-Quads as `tools/golden/chunker.go` writes them, and the golden files it writes.

use serde::Deserialize;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::value::Val;
use ultragraphdb_test_bdd::{Golden, load_golden, testdata_dir};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct ValueJson {
    pub kind: String,
    pub str: String,
    pub hex: String,
    pub int: i64,
    pub bool: bool,
    pub uid: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct FacetJson {
    pub key: String,
    pub value: String,
    pub val_type: i32,
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct NQuadJson {
    pub subject: String,
    pub predicate: String,
    pub object_id: String,
    pub object_value: Option<ValueJson>,
    pub lang: String,
    pub facets: Vec<FacetJson>,
    pub namespace: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RdfCase {
    pub input: String,
    pub input_hex: String,
    pub valid_utf8: bool,
    pub empty: bool,
    pub err: String,
    pub panic: String,
    pub nquad: Option<NQuadJson>,
    pub table_expected_err: bool,
    pub table_should_ignore: bool,
    pub from_table: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HintJson {
    pub pred: String,
    pub hint: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RdfsCase {
    pub name: String,
    pub input: String,
    pub err: String,
    pub nquads: Vec<NQuadJson>,
    pub hints: Vec<HintJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonCase {
    pub name: String,
    pub input: String,
    pub op: i32,
    pub err: String,
    pub errs: Vec<String>,
    pub err_json: bool,
    pub nquads: Vec<NQuadJson>,
    pub hints: Vec<HintJson>,
    pub stable: bool,
    pub blanks: usize,
}

pub fn golden<T: for<'de> Deserialize<'de>>(name: &str) -> Golden<T> {
    load_golden(&testdata_dir().join(format!("golden/{name}.json")))
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// `_:dg.<id>.<n>` becomes `_:dg.<n>`, as the generator writes it.
pub fn without_blank_id(s: &str) -> String {
    match s
        .strip_prefix("_:dg.")
        .and_then(|rest| rest.split_once('.'))
    {
        Some((id, n)) if id.bytes().all(|b| b.is_ascii_digit()) => format!("_:dg.{n}"),
        _ => s.to_string(),
    }
}

/// `_:dg.<n>` becomes `_:dg`, so that results compare whatever order the blank nodes were numbered in.
pub fn without_blank_counter(s: &str) -> String {
    if s.starts_with("_:dg.") {
        "_:dg".to_string()
    } else {
        s.to_string()
    }
}

fn value_json(v: &api::Value) -> ValueJson {
    let mut out = ValueJson {
        kind: String::new(),
        str: String::new(),
        hex: String::new(),
        int: 0,
        bool: false,
        uid: 0,
    };
    let kind = match &v.val {
        Some(Val::DefaultVal(s)) => {
            out.str = s.clone();
            "default"
        }
        Some(Val::StrVal(s)) => {
            out.str = s.clone();
            "str"
        }
        Some(Val::PasswordVal(s)) => {
            out.str = s.clone();
            "password"
        }
        Some(Val::IntVal(i)) => {
            out.int = *i;
            "int"
        }
        Some(Val::BoolVal(b)) => {
            out.bool = *b;
            "bool"
        }
        Some(Val::UidVal(u)) => {
            out.uid = *u;
            "uid"
        }
        Some(Val::DoubleVal(f)) => {
            out.hex = format!("{:016x}", f.to_bits());
            "double"
        }
        Some(Val::BytesVal(b)) => {
            out.hex = hex(b);
            "bytes"
        }
        Some(Val::GeoVal(b)) => {
            out.hex = hex(b);
            "geo"
        }
        Some(Val::DateVal(b)) => {
            out.hex = hex(b);
            "date"
        }
        Some(Val::DatetimeVal(b)) => {
            out.hex = hex(b);
            "datetime"
        }
        Some(Val::BigfloatVal(b)) => {
            out.hex = hex(b);
            "bigfloat"
        }
        Some(Val::Vfloat32Val(b)) => {
            out.hex = hex(b);
            "vfloat32"
        }
        None => "none",
    };
    out.kind = kind.to_string();
    out
}

/// The N-Quad as the generator writes it: the blank-node id dropped, facets sorted by key when `sort_facets`.
pub fn nquad_json(nq: &api::NQuad, sort_facets: bool) -> NQuadJson {
    let mut facets: Vec<FacetJson> = nq
        .facets
        .iter()
        .map(|f| FacetJson {
            key: f.key.clone(),
            value: hex(&f.value),
            val_type: f.val_type,
            tokens: f.tokens.iter().map(|t| hex(t.as_bytes())).collect(),
        })
        .collect();
    if sort_facets {
        facets.sort_by(|a, b| a.key.cmp(&b.key));
    }
    NQuadJson {
        subject: without_blank_id(&nq.subject),
        predicate: nq.predicate.clone(),
        object_id: without_blank_id(&nq.object_id),
        object_value: nq.object_value.as_ref().map(value_json),
        lang: nq.lang.clone(),
        facets,
        namespace: nq.namespace,
    }
}

/// N-Quads sorted, without blank-node counters: what does not depend on Go's map iteration.
pub fn canonical(nqs: impl IntoIterator<Item = NQuadJson>) -> Vec<NQuadJson> {
    let mut out: Vec<NQuadJson> = nqs
        .into_iter()
        .map(|mut nq| {
            nq.subject = without_blank_counter(&nq.subject);
            nq.object_id = without_blank_counter(&nq.object_id);
            nq
        })
        .collect();
    out.sort();
    out
}
