/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_chunker::{ChunkerErrorEnum, JsonOp, parse_json, parse_rdf};
use ultragraphdb_core_lex::Lexer;

fn rdf_err(line: &str) -> ultragraphdb_core_chunker::ChunkerError {
    parse_rdf(line, &mut Lexer::new(line)).unwrap_err()
}

fn json_err(input: &str, op: JsonOp) -> ultragraphdb_core_chunker::ChunkerError {
    parse_json(input.as_bytes(), op).unwrap_err()
}

#[test]
fn each_failure_has_its_kind() {
    assert_eq!(rdf_err("# c").kind(), &ChunkerErrorEnum::Empty);
    assert!(matches!(rdf_err("<a").kind(), ChunkerErrorEnum::Lex(_)));
    assert_eq!(
        rdf_err("<a> <p> .").kind(),
        &ChunkerErrorEnum::Rdf("invalid end of input. Input: [<a> <p> .]".to_string())
    );
    assert!(matches!(
        rdf_err(r#"<a> <p> "x"^^<xs:int> ."#).kind(),
        ChunkerErrorEnum::Types(_)
    ));
    assert!(matches!(
        json_err("{", JsonOp::Set).kind(),
        ChunkerErrorEnum::Json(_)
    ));
    assert_eq!(
        json_err(r#"{"name": "x"}"#, JsonOp::Delete).kind(),
        &ChunkerErrorEnum::EmptyUid
    );
    assert_eq!(
        json_err(r#"{"uid": "-1"}"#, JsonOp::Set).kind(),
        &ChunkerErrorEnum::JsonMutation(
            r#"strconv.ParseUint: parsing "-1": invalid syntax"#.to_string()
        )
    );
}

#[test]
fn each_failure_has_dgraph_text() {
    assert_eq!(
        rdf_err("# c").to_string(),
        "RDF: harmless error, e.g. comment line"
    );
    assert_eq!(
        rdf_err(r#"<a> <p> "x"^^<xs:int> ."#).to_string(),
        r#"strconv.ParseInt: parsing "x": invalid syntax"#
    );
    assert_eq!(
        rdf_err(r#"<a> <p> "\'" ."#).to_string(),
        "while unquoting: invalid syntax"
    );
    assert_eq!(
        rdf_err("<a> <p> <b> (k=abc) .").to_string(),
        "could not parse facet: Could not parse the facet value : [abc]"
    );
    assert_eq!(
        rdf_err("<a> <-> <b> .").to_string(),
        r#"NQuad failed sanity check. Subject: "a", Predicate: "-", ObjectId: "b""#
    );
    assert_eq!(
        json_err(r#"{"name": "x"}"#, JsonOp::Delete).to_string(),
        "UID must be present and non-zero while deleting edges"
    );
    assert_eq!(
        json_err("[1]", JsonOp::Set).to_string(),
        "only array of map allowed at root"
    );
    assert_eq!(json_err("", JsonOp::Set).to_string(), "EOF");
}

#[test]
fn the_error_is_a_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(rdf_err("<a> <p> ."));
    assert_eq!(err.to_string(), "invalid end of input. Input: [<a> <p> .]");
}
