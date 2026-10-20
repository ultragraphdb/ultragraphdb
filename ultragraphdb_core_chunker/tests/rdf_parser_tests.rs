/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/rdf_parser_test.go. Changed for ultragraphdb: translated to Rust; the table
 * written out from the Go values of testNQuads, with an empty facet value type spelt out as String.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_chunker::{ChunkerErrorEnum, parse_rdf, parse_rdfs};
use ultragraphdb_core_lex::Lexer;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;
use ultragraphdb_proto_dgraph::api::value::Val;
use ultragraphdb_proto_pb::pb::metadata::HintType;

struct TestCase {
    input: &'static str,
    nq: Option<api::NQuad>,
    expected_err: bool,
    should_ignore: bool,
}

fn value(val: Val) -> api::Value {
    api::Value { val: Some(val) }
}

fn default(s: &str) -> api::Value {
    value(Val::DefaultVal(s.to_string()))
}

fn str(s: &str) -> api::Value {
    value(Val::StrVal(s.to_string()))
}

fn int(i: i64) -> api::Value {
    value(Val::IntVal(i))
}

fn password(s: &str) -> api::Value {
    value(Val::PasswordVal(s.to_string()))
}

fn facet(key: &str, value: &[u8], val_type: ValType, tokens: &[&str]) -> api::Facet {
    api::Facet {
        key: key.to_string(),
        value: value.to_vec(),
        val_type: val_type as i32,
        tokens: tokens.iter().map(|t| t.to_string()).collect(),
        alias: String::new(),
    }
}

fn test_nquads() -> Vec<TestCase> {
    vec![
        TestCase {
            input: "<some_subject_id> <predicate> <object_id> .",
            nq: Some(api::NQuad {
                subject: "some_subject_id".into(),
                predicate: "predicate".into(),
                object_id: "object_id".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<some_subject_id>\u{9}<predicate>\u{9}<object_id>\u{9}.",
            nq: Some(api::NQuad {
                subject: "some_subject_id".into(),
                predicate: "predicate".into(),
                object_id: "object_id".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <predicate> <object_id> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "predicate".into(),
                object_id: "object_id".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<0x01> <predicate> <object_id> .",
            nq: Some(api::NQuad {
                subject: "0x01".into(),
                predicate: "predicate".into(),
                object_id: "object_id".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<some_subject_id> <predicate> <0x01> .",
            nq: Some(api::NQuad {
                subject: "some_subject_id".into(),
                predicate: "predicate".into(),
                object_id: "0x01".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<0x01> <predicate> <0x02> .",
            nq: Some(api::NQuad {
                subject: "0x01".into(),
                predicate: "predicate".into(),
                object_id: "0x02".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <follows> _:bob0 .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "follows".into(),
                object_id: "_:bob0".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <name> \"Alice In Wonderland\" .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "name".into(),
                object_id: "".into(),
                object_value: Some(default("Alice In Wonderland")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <name> \"Alice In Wonderland\"@en-0 .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "name".into(),
                object_id: "".into(),
                object_value: Some(default("Alice In Wonderland")),
                lang: "en-0".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <name@en> \"Alice In Wonderland\" .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "name".into(),
                object_id: "".into(),
                object_value: Some(default("Alice In Wonderland")),
                lang: "en".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <name@en> * .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "name".into(),
                object_id: "".into(),
                object_value: Some(default("_STAR_ALL")),
                lang: "en".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <name> \"Alice In Wonderland\"^^<xs:string> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "name".into(),
                object_id: "".into(),
                object_value: Some(str("Alice In Wonderland")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <age> \"013\"^^<xs:int> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "age".into(),
                object_id: "".into(),
                object_value: Some(int(13)),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <age> \"013\"^^<xs:integer> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "age".into(),
                object_id: "".into(),
                object_value: Some(int(13)),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <secret> \"password1\"^^<xs:password> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "secret".into(),
                object_id: "".into(),
                object_value: Some(password("password1")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<http://www.w3.org/2001/sw/RDFCore/nedges/> <http://purl.org/dc/terms/title> \"N-Edges\"@en-US .",
            nq: Some(api::NQuad {
                subject: "http://www.w3.org/2001/sw/RDFCore/nedges/".into(),
                predicate: "http://purl.org/dc/terms/title".into(),
                object_id: "".into(),
                object_value: Some(default("N-Edges")),
                lang: "en-US".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:art <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .",
            nq: Some(api::NQuad {
                subject: "_:art".into(),
                predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".into(),
                object_id: "http://xmlns.com/foaf/0.1/Person".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice knows .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<_:alice> <knows> <something> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "something".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> <_:something> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "_:something".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <knows> * .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("_STAR_ALL")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> * * .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "_STAR_ALL".into(),
                object_id: "".into(),
                object_value: Some(default("_STAR_ALL")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <knows> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: " 0x01 <knows> <something> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <knows>  0x01 .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice \"knows\" stuff .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> stuff .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> <stuff>",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "\"_:alice\" <knows> <stuff> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"@-en .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^<string> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^xs:string .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <age> \"thirteen\"^^<xs:int> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <knows> <*> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<*> <knows> \"stuff\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <*> \"stuff\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> < * > \"stuff\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <* *> \"stuff\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <*> \"stuff\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^< * > .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"\" .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"\"^^<xs:string> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(str("")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"\"^^<xs:int> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <knows> \"*\" .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("*")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> <0xf2> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(str("stuff")),
                lang: "".into(),
                facets: vec![],
                namespace: 0xf2,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> <0xf2> .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(str("stuff")),
                lang: "".into(),
                facets: vec![],
                namespace: 0xf2,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> <10> . # comment",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(str("stuff")),
                lang: "".into(),
                facets: vec![],
                namespace: 0xa,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> \"0xf2\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> 0x01 .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> <quad> <pentagon> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> quad .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\"^^<xs:string> <*> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> <bob> . <bob>",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <likes> \"mov\\\"enpick\" .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "likes".into(),
                object_id: "".into(),
                object_value: Some(default("mov\"enpick")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<\\u0021> <\\U123abcdE> <\\u0024> .",
            nq: Some(api::NQuad {
                subject: "\\u0021".into(),
                predicate: "\\U123abcdE".into(),
                object_id: "\\u0024".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<\\u0021> <\\U123abcdg> <\\u0024> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<messi with space> <friend> <ronaldo> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<with<> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<wi>th> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<\"with> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<{with> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<wi{th> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<with|> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<wit^h> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<w`ith> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<wi\\th> <with> <with> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:|alice <abc> <abc> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:al\u{d7}ice <abc> <abc> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:gabe <name> \"Gabe' .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:gabe <name> \"Gabe'^^<xs:yo> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:0 <name> <good> .",
            nq: Some(api::NQuad {
                subject: "_:0".into(),
                predicate: "name".into(),
                object_id: "good".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:0a.b <name> <good> .",
            nq: Some(api::NQuad {
                subject: "_:0a.b".into(),
                predicate: "name".into(),
                object_id: "good".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:0a. <name> <bad> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"wonder \\a land\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"\\u0045 wonderland\" .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("E wonderland")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"wonderland\" (friend=\"hatter\").",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("wonderland")),
                lang: "".into(),
                facets: vec![facet(
                    "friend",
                    b"hatter",
                    ValType::String,
                    &["\u{1}hatter"],
                )],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"wonderland\" (friend=\"hatter \\u0045\") .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("wonderland")),
                lang: "".into(),
                facets: vec![facet(
                    "friend",
                    b"hatter E",
                    ValType::String,
                    &["\u{1}e", "\u{1}hatter"],
                )],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"\\u004 wonderland\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"\\x02 wonderland\" .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("\u{2} wonderland")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"\\x2 wonderland\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"wonderful land\"@a- .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"\\v\\t\\b\\n\\r\\f\\\"\\\\\"@a-b .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("\u{b}\u{9}\u{8}\u{a}\u{d}\u{c}\"\\")),
                lang: "a-b".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"\\'\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"\\a\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "# nothing happened",
            nq: None,
            expected_err: true,
            should_ignore: true,
        },
        TestCase {
            input: "<some_subject_id> # <predicate> <object_id> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<some_subject_id> <predicate> <object_id> # .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "check me as error",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "   ",
            nq: None,
            expected_err: true,
            should_ignore: true,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" <0x10> (key1=\"val1\",key2=13) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![
                    facet("key1", b"val1", ValType::String, &["\u{1}val1"]),
                    facet(
                        "key2",
                        b"\x0d\x00\x00\x00\x00\x00\x00\x00",
                        ValType::Int,
                        &[],
                    ),
                ],
                namespace: 0x10,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" <0x12> (key1=,key2=13) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![
                    facet("key1", b"", ValType::String, &["\u{1}"]),
                    facet(
                        "key2",
                        b"\x0d\x00\x00\x00\x00\x00\x00\x00",
                        ValType::Int,
                        &[],
                    ),
                ],
                namespace: 0x12,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=,key2=13) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![
                    facet("key1", b"", ValType::String, &["\u{1}"]),
                    facet(
                        "key2",
                        b"\x0d\x00\x00\x00\x00\x00\x00\x00",
                        ValType::Int,
                        &[],
                    ),
                ],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" ( key1 = 12 , key2=\"value2\", key3=, key4 =\"val4\" ) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![
                    facet(
                        "key1",
                        b"\x0c\x00\x00\x00\x00\x00\x00\x00",
                        ValType::Int,
                        &[],
                    ),
                    facet("key2", b"value2", ValType::String, &["\u{1}value2"]),
                    facet("key3", b"", ValType::String, &["\u{1}"]),
                    facet("key4", b"val4", ValType::String, &["\u{1}val4"]),
                ],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=12,key2=\"value2\",key3=1.2,key4=2006-01-02T15:04:05,key5=true,key6=false) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![
                    facet(
                        "key1",
                        b"\x0c\x00\x00\x00\x00\x00\x00\x00",
                        ValType::Int,
                        &[],
                    ),
                    facet("key2", b"value2", ValType::String, &["\u{1}value2"]),
                    facet("key3", b"333333\xf3?", ValType::Float, &[]),
                    facet(
                        "key4",
                        b"\x01\x00\x00\x00\x0e\xbbK7\xe5\x00\x00\x00\x00\xff\xff",
                        ValType::Datetime,
                        &[],
                    ),
                    facet("key5", b"\x01", ValType::Bool, &[]),
                    facet("key6", b"\x00", ValType::Bool, &[]),
                ],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=2002-10-02T15:00:00.05Z, key2=2006-01-02T15:04:05, key3=2006-01-02T00:00:00Z) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![
                    facet(
                        "key1",
                        b"\x01\x00\x00\x00\x0e\xb5-\x00\xf0\x02\xfa\xf0\x80\xff\xff",
                        ValType::Datetime,
                        &[],
                    ),
                    facet(
                        "key2",
                        b"\x01\x00\x00\x00\x0e\xbbK7\xe5\x00\x00\x00\x00\xff\xff",
                        ValType::Datetime,
                        &[],
                    ),
                    facet(
                        "key3",
                        b"\x01\x00\x00\x00\x0e\xbbJd\x00\x00\x00\x00\x00\xff\xff",
                        ValType::Datetime,
                        &[],
                    ),
                ],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (k=0x0D) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![facet(
                    "k",
                    b"\x0d\x00\x00\x00\x00\x00\x00\x00",
                    ValType::Int,
                    &[],
                )],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (k=111111111111111111888888.23) .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![facet("k", b"\xa0\xa8OlX\x87\xb7D", ValType::Float, &[])],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"\\\"hello world\\\"\",key2=\"LineA\\nLineB\") .",
            nq: Some(api::NQuad {
                subject: "_:alice".into(),
                predicate: "knows".into(),
                object_id: "".into(),
                object_value: Some(default("stuff")),
                lang: "".into(),
                facets: vec![
                    facet(
                        "key1",
                        b"\"hello world\"",
                        ValType::String,
                        &["\u{1}hello", "\u{1}world"],
                    ),
                    facet(
                        "key2",
                        b"LineA\x0aLineB",
                        ValType::String,
                        &["\u{1}linea", "\u{1}lineb"],
                    ),
                ],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"val1\",key2) .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"val1\",=) .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"val1\",=\"val1\") .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"val1\",key1 \"val1\") .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"val1\",key1= \"val1\" .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"val1\",key1= .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (key1=\"val1\",key1=",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (k==)",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (k=,) .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (k=111111111111111111888888) .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:alice <knows> \"stuff\" (k=0x1787586C4FA8A0284FF8) .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <password> \"guess123\"^^<pwd:password> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "* <pred> * .",
            nq: Some(api::NQuad {
                subject: "_STAR_ALL".into(),
                predicate: "pred".into(),
                object_id: "".into(),
                object_value: Some(default("_STAR_ALL")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "* <pred> \"random\"^^<int> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "_:company <name> \"TurfBytes\" . _:company <owner> _:owner . _:owner <name> \"Jason\" .  ",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <lives> \"A\\tB\" .",
            nq: Some(api::NQuad {
                subject: "alice".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("A\u{9}B")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <age> \"NaN\"^^<xs:double> .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "<alice> <age> \"13\"^^<xs:double> (salary=NaN) .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "uid(v) <lives> \"\\x02 wonderland\" .",
            nq: Some(api::NQuad {
                subject: "uid(v)".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("\u{2} wonderland")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "uid  (  v  ) <lives> \"vrinadavan\" .",
            nq: Some(api::NQuad {
                subject: "uid(v)".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("vrinadavan")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "uid  (  val  ) <lives> \"vrinadavan\" .",
            nq: Some(api::NQuad {
                subject: "uid(val)".into(),
                predicate: "lives".into(),
                object_id: "".into(),
                object_value: Some(default("vrinadavan")),
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "uid  (  val  ) <lives> uid(g) .",
            nq: Some(api::NQuad {
                subject: "uid(val)".into(),
                predicate: "lives".into(),
                object_id: "uid(g)".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "uid  (  val  ) <lives> uid ( g )  .",
            nq: Some(api::NQuad {
                subject: "uid(val)".into(),
                predicate: "lives".into(),
                object_id: "uid(g)".into(),
                object_value: None,
                lang: "".into(),
                facets: vec![],
                namespace: 0x0,
            }),
            expected_err: false,
            should_ignore: false,
        },
        TestCase {
            input: "uid  (  val   <lives> uid ( g )  .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "uid   val )   <lives> uid ( g )  .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "ui(uid)   <lives> uid ( g )  .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "uid())   <lives> uid ( g )  .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "uid()   <lives> uid ( g )  .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "uid(a)   <lives> uid (  )  .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
        TestCase {
            input: "uid(a)   lives> uid (  )  .",
            nq: None,
            expected_err: true,
            should_ignore: false,
        },
    ]
}

#[test]
fn test_lex() {
    let cases = test_nquads();
    assert_eq!(cases.len(), 130);
    let mut l = Lexer::new("");
    for test in &cases {
        l.reset(test.input);
        let rnq = parse_rdf(test.input, &mut l);
        match (test.expected_err, test.should_ignore) {
            (true, true) => {
                let err = rnq.expect_err(test.input);
                assert_eq!(
                    err.kind(),
                    &ChunkerErrorEnum::Empty,
                    "Catch an ignorable case: {err}"
                );
            }
            (true, false) => {
                assert!(
                    rnq.is_err(),
                    "Expected error for input: {:?}. Output: {rnq:?}",
                    test.input
                );
            }
            _ => {
                let rnq =
                    rnq.unwrap_or_else(|e| panic!("Got error for input: {:?}: {e}", test.input));
                assert_eq!(
                    test.nq.as_ref(),
                    Some(&rnq),
                    "Mismatch for input: {:?}",
                    test.input
                );
            }
        }
    }
}

#[test]
fn an_empty_or_comment_line_is_harmless() {
    for line in ["", "   ", "\t", "# comment", "  # indented"] {
        let err = parse_rdf(line, &mut Lexer::new(line)).unwrap_err();
        assert_eq!(err.kind(), &ChunkerErrorEnum::Empty);
        assert_eq!(err.to_string(), "RDF: harmless error, e.g. comment line");
    }
}

#[test]
fn the_lexing_error_names_the_line_and_column() {
    let line = r#"_:alice <knows> "stuff"^^<xs:string> 0x01 ."#;
    let err = parse_rdf(line, &mut Lexer::new(line)).unwrap_err();
    assert!(matches!(err.kind(), ChunkerErrorEnum::Lex(_)));
    assert_eq!(
        err.to_string(),
        format!("while lexing {line} at line 1 column 36: Invalid input: 0 at lexText")
    );
}

#[test]
fn lexing_goes_on_after_an_invalid_rune_and_reports_the_first() {
    // lexText reports an unexpected rune without stopping, as Dgraph's does.
    let line = "<a> <p> \"x\" ! ? .";
    let err = parse_rdf(line, &mut Lexer::new(line)).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("while lexing {line} at line 1 column 11: Invalid input: ! at lexText")
    );
}

#[test]
fn parse_rdfs_skips_blank_lines_and_comments_and_hints_lists() {
    let input = b"<a> <p> \"x\" .\n\n# c\n<a> <q> \"y\" .\n<a> <q> \"z\" .\n<b> <p> \"w\" .\n";
    let (nqs, md) = parse_rdfs(input).unwrap();
    assert_eq!(nqs.len(), 4);
    assert_eq!(md.pred_hints.len(), 1);
    assert_eq!(md.pred_hints["q"], HintType::List as i32);
}

#[test]
fn parse_rdfs_stops_at_the_first_bad_line() {
    let err = parse_rdfs(b"<a> <p> \"x\" .\n<a> <p> .\n").unwrap_err();
    assert_eq!(err.to_string(), "invalid end of input. Input: [<a> <p> .]");
}

#[test]
fn parse_rdfs_of_nothing_is_nothing() {
    let (nqs, md) = parse_rdfs(b"").unwrap();
    assert!(nqs.is_empty());
    assert!(md.pred_hints.is_empty());
}

#[test]
fn bytes_that_are_not_utf8_become_replacement_characters() {
    // Go keeps the raw bytes of an IRI and replaces those of a literal while unquoting it; a Rust string cannot
    // hold raw bytes, so every byte that is not UTF-8 becomes U+FFFD before lexing.
    let (nqs, _) = parse_rdfs(b"<a\xff> <p> \"b\xe2\x82\" .").unwrap();
    assert_eq!(nqs[0].subject, "a\u{fffd}");
    assert_eq!(nqs[0].object_value, Some(default("b\u{fffd}\u{fffd}")));
    let err = parse_rdfs(b"\xff").unwrap_err();
    assert_eq!(
        err.to_string(),
        "while lexing \u{fffd} at line 1 column 0: Invalid input: \u{fffd} at lexText"
    );
}

#[test]
fn a_hex_escape_that_is_not_utf8_becomes_a_replacement_character() {
    let line = r#"<a> <p> "\xe2\x82\xac \xff" ."#;
    let nq = parse_rdf(line, &mut Lexer::new(line)).unwrap();
    assert_eq!(nq.object_value, Some(default("\u{20ac} \u{fffd}")));
}

#[test]
fn typed_literals_take_the_types_of_the_type_map() {
    let parse = |line: &str| parse_rdf(line, &mut Lexer::new(line)).map(|nq| nq.object_value);
    assert_eq!(
        parse(r#"<a> <p> "abc"^^<xs:string> ."#).unwrap(),
        Some(str("abc"))
    );
    assert_eq!(
        parse(r#"<a> <p> "13"^^<xs:positiveInteger> ."#).unwrap(),
        Some(int(13))
    );
    assert_eq!(
        parse(r#"<a> <p> "pw"^^<xs:password> ."#).unwrap(),
        Some(password("pw"))
    );
    assert_eq!(
        parse(r#"<a> <p> "1.5"^^<http://www.w3.org/2001/XMLSchema#double> ."#).unwrap(),
        Some(value(Val::DoubleVal(1.5)))
    );
    assert_eq!(
        parse(r#"<a> <p> "true"^^<xs:boolean> ."#).unwrap(),
        Some(value(Val::BoolVal(true)))
    );
    assert_eq!(
        parse(r#"<a> <p> "x"^^<xs:unknown> ."#)
            .unwrap_err()
            .to_string(),
        "unrecognized rdf type xs:unknown"
    );
    assert_eq!(
        parse(r#"<a> <p> ""^^<xs:int> ."#).unwrap_err().to_string(),
        "invalid ObjectValue"
    );
}
