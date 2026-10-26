/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::LiteralRef;
use oxsdatatypes::{Boolean, Date, DateTime, Decimal, Double, Float, Integer};
use ultragraphdb_core_catalog::tokenizer_takes;
use ultragraphdb_core_types::{TypeId, Val, Value, convert};

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// The value tokenizer `tokenizer` tokenizes `literal` as, computed from the literal's value so that lexical forms
/// of one value give one token: `None` when the tokenizer takes no literal of its datatype, or the lexical form is
/// not one of the datatype, and the literal is then held unindexed. A date-time without a time zone is taken as UTC.
pub fn token_value(literal: LiteralRef<'_>, tokenizer: &str) -> Option<Value> {
    let datatype = literal.datatype().as_str();
    if !tokenizer_takes(tokenizer, datatype) {
        return None;
    }
    let lexical = literal.value();
    let local = datatype.strip_prefix(XSD).unwrap_or_default();
    let text = |s: String, to: TypeId| {
        convert(&Val::new(TypeId::STRING, Value::Bytes(s.into_bytes())), to)
            .ok()
            .and_then(|v| v.value().cloned())
    };
    match tokenizer {
        "int" => lexical
            .parse::<Integer>()
            .ok()
            .map(|i| Value::Int(i64::from(i))),
        "float" if local == "float" => lexical
            .parse::<Float>()
            .ok()
            .map(|f| Value::Float(f64::from(f32::from(f)))),
        "float" => lexical
            .parse::<Double>()
            .ok()
            .map(|d| Value::Float(f64::from(d))),
        "bool" => lexical
            .parse::<Boolean>()
            .ok()
            .map(|b| Value::Bool(bool::from(b))),
        "bigfloat" => lexical
            .parse::<Decimal>()
            .ok()
            .and_then(|d| text(d.to_string(), TypeId::BIGFLOAT)),
        "year" | "month" | "day" | "hour" => {
            let canonical = if local == "date" {
                lexical.parse::<Date>().ok()?.to_string()
            } else {
                lexical.parse::<DateTime>().ok()?.to_string()
            };
            text(canonical, TypeId::DATETIME)
        }
        "geo" => text(lexical.to_string(), TypeId::GEO),
        _ => Some(Value::Str(lexical.to_string())),
    }
}
