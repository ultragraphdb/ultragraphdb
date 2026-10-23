/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, then, when};
use ultragraphdb_core_chunker::{
    ChunkerError, ChunkerErrorEnum, InputFormat, JsonOp, data_format, parse_json, parse_rdf,
};
use ultragraphdb_core_lex::Lexer;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::value::Val;

// Shared with the unit tests, which use all of it.
#[allow(dead_code)]
#[path = "golden_nquad.rs"]
mod golden_nquad;

use golden_nquad::{RdfCase, golden, nquad_json};

#[derive(Debug, Default, World)]
struct Steps {
    nquads: Vec<api::NQuad>,
    error: Option<ChunkerError>,
    mismatches: Vec<String>,
    compared: usize,
    format: Option<InputFormat>,
}

impl Steps {
    fn only(&self) -> &api::NQuad {
        assert!(self.error.is_none(), "{:?}", self.error);
        assert_eq!(self.nquads.len(), 1, "{:?}", self.nquads);
        &self.nquads[0]
    }

    fn find(&self, subject: &str, predicate: &str) -> &api::NQuad {
        self.nquads
            .iter()
            .find(|nq| nq.subject == subject && nq.predicate == predicate)
            .unwrap_or_else(|| panic!("no {subject} {predicate} in {:?}", self.nquads))
    }
}

/// The text between the first and the last backtick.
fn quoted(text: &str) -> &str {
    let start = text.find('`').expect("`") + 1;
    let end = text.rfind('`').expect("`");
    &text[start..end]
}

#[when("the 130 cases of Dgraph's RDF parser table are parsed")]
fn rdf_table(s: &mut Steps) {
    let g = golden::<RdfCase>("chunker_rdf");
    let table: Vec<&RdfCase> = g.cases().iter().filter(|c| c.from_table).collect();
    assert_eq!(table.len(), 130);
    let mut l = Lexer::new("");
    for c in table {
        s.compared += 1;
        let got = parse_rdf(&c.input, &mut l);
        let ok = match (&c.nquad, &got) {
            (Some(want), Ok(nq)) => *want == nquad_json(nq, false),
            (None, Err(e)) if c.empty => matches!(e.0, ChunkerErrorEnum::Empty),
            (None, Err(e)) => e.to_string() == c.err,
            _ => false,
        };
        if !ok {
            s.mismatches.push(format!(
                "{:?}: {got:?}, want {:?} / {:?}",
                c.input, c.nquad, c.err
            ));
        }
    }
}

#[then("each gives Dgraph's N-Quad or Dgraph's error")]
fn rdf_table_matches(s: &mut Steps) {
    assert_eq!(s.compared, 130);
    assert!(s.mismatches.is_empty(), "{}", s.mismatches.join("\n"));
}

#[when(regex = r"^the RDF line (`.+`) is parsed$")]
fn rdf_line(s: &mut Steps, text: String) {
    let line = quoted(&text);
    match parse_rdf(line, &mut Lexer::new(line)) {
        Ok(nq) => s.nquads.push(nq),
        Err(e) => s.error = Some(e),
    }
}

#[then(regex = r"^the object value is the int (\d+)$")]
fn object_int(s: &mut Steps, n: i64) {
    assert_eq!(
        s.only().object_value,
        Some(api::Value {
            val: Some(Val::IntVal(n))
        })
    );
}

#[then(regex = r"^the N-Quad's language is `(\w+)`$")]
fn language(s: &mut Steps, lang: String) {
    assert_eq!(s.only().lang, lang);
}

#[then(regex = r"^the predicate and the object value are `(\w+)`$")]
fn star_form(s: &mut Steps, star: String) {
    let nq = s.only();
    assert_eq!(nq.predicate, star);
    assert_eq!(
        nq.object_value,
        Some(api::Value {
            val: Some(Val::DefaultVal(star))
        })
    );
}

#[then("parsing fails")]
fn parsing_fails(s: &mut Steps) {
    assert!(s.error.is_some(), "{:?}", s.nquads);
}

#[when(regex = r"^the JSON (`.+`) is parsed$")]
fn json(s: &mut Steps, text: String) {
    match parse_json(quoted(&text).as_bytes(), JsonOp::Set) {
        Ok((nqs, _)) => s.nquads = nqs,
        Err(e) => s.error = Some(e),
    }
}

#[when(regex = r"^(`.+`) is parsed as a (set|delete) mutation$")]
fn json_mutation(s: &mut Steps, text: String, op: String) {
    let op = if op == "set" {
        JsonOp::Set
    } else {
        JsonOp::Delete
    };
    match parse_json(quoted(&text).as_bytes(), op) {
        Ok((nqs, _)) => s.nquads = nqs,
        Err(e) => s.error = Some(e),
    }
}

#[then(regex = r"^parsing fails with (`.+`)$")]
fn parsing_fails_with(s: &mut Steps, text: String) {
    let e = s.error.as_ref().unwrap_or_else(|| panic!("{:?}", s.nquads));
    assert_eq!(e.to_string(), quoted(&text));
}

#[then(regex = r"^parsing fails with (`.+`) and nothing panics$")]
fn parsing_fails_without_panic(s: &mut Steps, text: String) {
    // A panic would have failed the When step; the line is refused with the text.
    parsing_fails_with(s, text);
}

#[then(regex = r"^both N-Quads have predicate `(\S+)` and language `(\S+)`$")]
fn predicate_and_language(s: &mut Steps, predicate: String, lang: String) {
    assert!(s.error.is_none(), "{:?}", s.error);
    assert_eq!(s.nquads.len(), 2, "{:?}", s.nquads);
    for nq in &s.nquads {
        assert_eq!(
            (nq.predicate.as_str(), nq.lang.as_str()),
            (predicate.as_str(), lang.as_str())
        );
    }
}

#[then("the N-Quad's object is the float 0, not `*`")]
fn float_zero(s: &mut Steps) {
    assert_eq!(
        s.only().object_value,
        Some(api::Value {
            val: Some(Val::DoubleVal(0.0))
        })
    );
}

#[when(regex = r"^the format of `(.+)` with the option `(.+)` is decided$")]
fn format_decided(s: &mut Steps, filename: String, option: String) {
    s.format = Some(data_format(&filename, &option));
}

#[then("it is JSON")]
fn it_is_json(s: &mut Steps) {
    assert_eq!(s.format, Some(InputFormat::Json));
}

#[then("the result holds `name` for both nodes and a `friend` edge from `_:a` to `_:b`")]
fn nested(s: &mut Steps) {
    assert!(s.error.is_none(), "{:?}", s.error);
    assert_eq!(s.nquads.len(), 3);
    let name = |nq: &api::NQuad| nq.object_value.clone().and_then(|v| v.val);
    assert_eq!(name(s.find("_:a", "name")), Some(Val::StrVal("A".into())));
    assert_eq!(name(s.find("_:b", "name")), Some(Val::StrVal("B".into())));
    assert_eq!(s.find("_:a", "friend").object_id, "_:b");
}

#[then("the `friend` edge carries the facet `close` with bool value true")]
fn edge_facet(s: &mut Steps) {
    assert!(s.error.is_none(), "{:?}", s.error);
    let edge = s.find("_:a", "friend");
    assert_eq!(edge.object_id, "_:b");
    assert_eq!(edge.facets.len(), 1);
    assert_eq!(edge.facets[0].key, "close");
    assert_eq!(edge.facets[0].val_type(), api::facet::ValType::Bool);
    assert_eq!(edge.facets[0].value, [1]);
}

#[then(
    "each node is assigned `_:dg.<id>.<n>`, with one random u32 `<id>` and consecutive counters `<n>` from 1 up"
)]
fn blank_nodes(s: &mut Steps) {
    assert!(s.error.is_none(), "{:?}", s.error);
    let blanks: Vec<(u32, u64)> = s
        .nquads
        .iter()
        .map(|nq| {
            let rest = nq.subject.strip_prefix("_:dg.").expect("a blank node");
            let (id, n) = rest.split_once('.').expect("_:dg.<id>.<n>");
            (id.parse().expect("a u32 id"), n.parse().expect("a counter"))
        })
        .collect();
    assert_eq!(blanks.len(), 2);
    assert_eq!(blanks[0].0, blanks[1].0, "one id per process");
    assert!(blanks[0].1 >= 1);
    // The objects of a list are numbered in order; the counter is shared by the whole process.
    assert_eq!(blanks[1].1, blanks[0].1 + 1);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
