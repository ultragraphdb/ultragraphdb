/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The cases of parse_test.go that call `Parse`; those loading the state through `ParseBytes` are in
//! types/schema_state.

use ultragraphdb_core_catalog::{
    ParsedSchema, SchemaError, has_tokenizer_or_vector_index_spec, parse, parse_with_namespace,
};
use ultragraphdb_core_keys::{attr_in_root_namespace, namespace_attr};
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

fn su(pred: &str, value_type: i32) -> pb::SchemaUpdate {
    pb::SchemaUpdate {
        predicate: pred.to_string(),
        value_type,
        ..Default::default()
    }
}

fn indexed(mut u: pb::SchemaUpdate, tokenizers: &[&str]) -> pb::SchemaUpdate {
    u.set_directive(Directive::Index);
    u.tokenizer = tokenizers.iter().map(|t| t.to_string()).collect();
    u
}

fn listed(mut u: pb::SchemaUpdate) -> pb::SchemaUpdate {
    u.list = true;
    u
}

fn field(pred: &str) -> pb::SchemaUpdate {
    su(pred, 0)
}

fn typ(name: &str, fields: &[&str]) -> pb::TypeUpdate {
    pb::TypeUpdate {
        type_name: name.to_string(),
        fields: fields.iter().map(|f| field(f)).collect(),
    }
}

fn ok(s: &str) -> ParsedSchema {
    parse(s).unwrap_or_else(|e| panic!("parsing {s:?}: {e}"))
}

fn err(s: &str) -> SchemaError {
    match parse(s) {
        Ok(res) => panic!("parsing {s:?} should fail, got {res:?}"),
        Err(e) => e,
    }
}

fn root(attr: &str) -> String {
    attr_in_root_namespace(attr)
}

#[test]
fn test_parse() {
    err("age:int @index . name:string");
}

#[test]
fn test_parse2() {
    let result = ok("");
    assert!(result.preds().is_empty());
}

#[test]
fn test_parse3_error() {
    err("age:uid @index .");
}

#[test]
fn test_parse4_error() {
    let e = err("alive:bool @index(geo) .");
    assert!(
        e.to_string()
            .contains("Tokenizer: geo isn't valid for predicate: alive of type: bool")
    );
}

#[test]
fn test_parse4_no_error() {
    ok("name:string @index(fulltext) .");
}

#[test]
fn test_parse5_error() {
    err("value:default @index .");
}

#[test]
fn test_parse6_error() {
    err("pass:password @index .");
}

#[test]
fn test_parse7_error() {
    err("name:string @index .");
}

#[test]
fn test_parse8_error() {
    err("dob:dateTime @index .");
}

#[test]
fn test_parse9_error() {
    ok("age:uid @noconflict .");
}

#[test]
fn test_parse_scalar_list() {
    let result = ok("
		jobs: [string] @index(term) .
		occupations: [string] .
		graduation: [dateTime] .
	");
    assert_eq!(result.preds().len(), 3);
    assert_eq!(
        result.preds()[0],
        listed(indexed(su(&root("jobs"), 9), &["term"]))
    );
    assert_eq!(result.preds()[1], listed(su(&root("occupations"), 9)));
    assert_eq!(result.preds()[2], listed(su(&root("graduation"), 5)));
}

#[test]
fn test_parse_scalar_list_error1() {
    let e = err("
		friend: [string .
	");
    assert!(
        e.to_string()
            .contains("Unclosed [ while parsing schema for: friend")
    );
}

#[test]
fn test_parse_scalar_list_error2() {
    let e = err("
		friend: string] .
	");
    assert!(e.to_string().contains("Invalid ending"));
}

#[test]
fn test_parse_scalar_list_error3() {
    let e = err("
		friend: [bool] .
	");
    assert!(e.to_string().contains("Unsupported type for list: [bool]"));
}

#[test]
fn test_parse_uid_list() {
    let result = ok("
		friend: [uid] .
	");
    assert_eq!(result.preds(), [listed(su(&root("friend"), 7))]);
}

#[test]
fn test_parse_uid_single_value() {
    let result = ok("
		friend: uid .
	");
    assert_eq!(result.preds(), [su(&root("friend"), 7)]);
}

#[test]
fn test_parse_underscore() {
    ok("_share_:string @index(term) .");
}

#[test]
fn test_parse_upsert() {
    ok("
		jobs : string @index(exact) @upsert .
		age  : int @index(int) @upsert .
	");
}

#[test]
fn test_parse_empty_type() {
    let result = ok("
		type Person {

		}
	");
    assert_eq!(result.types(), [typ(&root("Person"), &[])]);
}

#[test]
fn test_parse_type_eof() {
    let result = ok("
		type Person {
		}");
    assert_eq!(result.types(), [typ(&root("Person"), &[])]);
}

#[test]
fn test_parse_single_type() {
    let result = ok("
		type Person {
			name
		}
	");
    assert_eq!(result.types(), [typ(&root("Person"), &[&root("name")])]);
}

#[test]
fn test_parse_combined_schemas_and_types() {
    let result = ok("
		type Person {
			name
		}
        name: string .
	");
    assert_eq!(result.preds(), [su(&root("name"), 9)]);
    assert_eq!(result.types(), [typ(&root("Person"), &[&root("name")])]);
}

#[test]
fn test_parse_multiple_types() {
    let result = ok("
		type Person {
			name
		}
		type Animal {
			name
		}
	");
    assert_eq!(
        result.types(),
        [
            typ(&root("Person"), &[&root("name")]),
            typ(&root("Animal"), &[&root("name")])
        ]
    );
}

#[test]
fn test_parse_type_duplicate_fields() {
    let e = err("
		type Person {
			name
			name
		}
	");
    assert!(e.to_string().contains("Duplicate fields with name: name"));
}

#[test]
fn test_old_type_format() {
    let result = ok("
		type Person {
			name: [string!]!
			address: string!
			children: [Person]
		}
	");
    assert_eq!(
        result.types(),
        [typ(
            &root("Person"),
            &[&root("name"), &root("address"), &root("children")]
        )]
    );
}

#[test]
fn test_old_and_new_type_format() {
    let result = ok("
		type Person {
			name: [string!]!
			address
		}
	");
    assert_eq!(
        result.types(),
        [typ(&root("Person"), &[&root("name"), &root("address")])]
    );
}

#[test]
fn test_parse_type_err_missing_new_line() {
    let e = err("
		type Person {
		}type Animal {}");
    assert!(
        e.to_string()
            .contains("Expected new line or EOF after type declaration")
    );
}

#[test]
fn test_parse_comments() {
    ok("
		#
		# This is a test
		#
		user: bool .
		user.name: string @index(exact) . # this should be unique
		user.email: string @index(exact) . # this should be unique and lower-cased
		user.password: password .
		user.code: string . # for password recovery (can be null)
		node: bool .
		node.hashid: string @index(exact) . # @username/hashid
		node.owner: uid @reverse . # (can be null)
		node.parent: uid . # [uid] (use facet)
		node.xdata: string . # store custom json data
		#
		# End of test
		#
	");
}

#[test]
fn test_parse_comments_noop() {
    ok("
# Spicy jalapeno bacon ipsum dolor amet t-bone kevin spare ribs sausage jowl cow pastrami short.
# Leberkas alcatra kielbasa chicken pastrami swine bresaola. Spare ribs landjaeger meatloaf.
# Chicken biltong boudin porchetta jowl swine burgdoggen cow kevin ground round landjaeger ham.
# Tongue buffalo cow filet mignon boudin sirloin pancetta pork belly beef ribs. Cow landjaeger.
	");
}

#[test]
fn test_parse_comments_err_missing_type() {
    let e = err("
		# The definition below should trigger an error
		# because we commented out its type.
		node: # bool .
	");
    assert!(e.to_string().contains("Missing Type"));
}

#[test]
fn test_parse_type_comments() {
    ok("
		# User is a service user
		type User {
			# TODO: add more fields
			name # e.g., srfrog
									 # expanded comment
									 # embedded # comments # here
		}
		# /User
	");
}

#[test]
fn test_parse_with_namespace() {
    let result = ok("
		[10] jobs: [string] @index(term) .
		[0x123] occupations: [string] .
		[0xf2] graduation: [dateTime] .
		[0xf2] type Person {
			name: [string!]!
			address: string!
			children: [Person]
		}
	");
    assert_eq!(
        result.preds(),
        [
            listed(indexed(su(&namespace_attr(10, "jobs"), 9), &["term"])),
            listed(su(&namespace_attr(0x123, "occupations"), 9)),
            listed(su(&namespace_attr(0xf2, "graduation"), 5)),
        ]
    );
    assert_eq!(
        result.types(),
        [typ(
            &namespace_attr(0xf2, "Person"),
            &[
                &namespace_attr(0xf2, "name"),
                &namespace_attr(0xf2, "address"),
                &namespace_attr(0xf2, "children"),
            ]
        )]
    );
}

// Rust-only cases below: the namespace override, the error positions and the vector index specification.

#[test]
fn parse_with_namespace_overrides_every_namespace() {
    let result = parse_with_namespace(
        "[0xf2] name: string .\nage: int .\n[1] type T {\n name\n}",
        5,
    )
    .expect("parses");
    assert_eq!(
        result.preds(),
        [
            su(&namespace_attr(5, "name"), 9),
            su(&namespace_attr(5, "age"), 2)
        ]
    );
    assert_eq!(
        result.types(),
        [typ(&namespace_attr(5, "T"), &[&namespace_attr(5, "name")])]
    );
}

#[test]
fn parse_with_namespace_max_preserves_namespaces() {
    let result = parse_with_namespace("[0xf2] name: string .", u64::MAX).expect("parses");
    assert_eq!(result.preds(), [su(&namespace_attr(0xf2, "name"), 9)]);
}

#[test]
fn errors_carry_dgraph_positions() {
    assert_eq!(
        err("name: string @index(exact, exact) .").to_string(),
        "line 1 column 25: Only one index tokenizer can be sortable for 0-name"
    );
    assert_eq!(
        err("friend: [string .").to_string(),
        "line 1 column 16: Unclosed [ while parsing schema for: friend"
    );
    assert_eq!(
        err("[").to_string(),
        "While parsing namespace:: Unable to peek: Out of range for peek"
    );
    assert_eq!(
        err("[0x5").to_string(),
        "While parsing namespace:: Typed oes not match the expected"
    );
}

#[test]
fn hnsw_index_parses_into_a_vector_index_spec() {
    let result = ok(r#"indexvector: float32vector @index(hnsw(metric:"euclidean")) ."#);
    let mut want = indexed(su(&root("indexvector"), 12), &[]);
    want.index_specs = vec![pb::VectorIndexSpec {
        name: "hnsw".to_string(),
        options: vec![pb::OptionPair {
            key: "metric".to_string(),
            value: "euclidean".to_string(),
        }],
    }];
    assert_eq!(result.preds(), [want]);
    assert!(has_tokenizer_or_vector_index_spec(&result.preds()[0]));
    assert!(!has_tokenizer_or_vector_index_spec(&su("0-x", 9)));
}

#[test]
fn hnsw_option_errors_are_the_factory_errors() {
    assert_eq!(
        err(r#"v: float32vector @index(hnsw(metric:"manhattan")) ."#).to_string(),
        "line 1 column 24: IndexFactory: hnsw issues this error: 'Can't create a vector index for manhattan'"
    );
}
