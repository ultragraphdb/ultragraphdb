/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/state_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_dql::{LEX_QUERY, LEX_TOP_LEVEL};
use ultragraphdb_core_lex::{ITEM_ERROR, ItemType, Lexer, StateFn};

/// The items the lexer emits from a start state, as type and text.
fn lex(input: &str, start: StateFn) -> Vec<(ItemType, String)> {
    let mut l = Lexer::new(input);
    l.run(start);
    let mut it = l.new_iterator();
    let mut items = Vec::new();
    while it.next() {
        items.push((it.item().typ(), it.item().val().to_string()));
    }
    items
}

fn assert_no_error(items: &[(ItemType, String)]) {
    for (typ, val) in items {
        assert_ne!(*typ, ITEM_ERROR, "Error: {val}");
    }
}

#[test]
fn test_query_lexing() {
    let input = r#"
	query {
		me(_xid_: rick, id:10 ) {
			name0 # my name
			_city, # 0what would fail lex.
			profilePic(width: 100, height: 100)
			friends {
				name
			}
		}
	}"#;
    let items = lex(input, LEX_QUERY);
    assert_no_error(&items);
}

#[test]
fn test_mutation_lexing() {
    let input = r#"
	mutation {
		set {
			What is <this> .
			Why is this #!!?
			How is this?
		}
		delete {
			Why is this
		}
	}
	query {
		me(_xid_: rick) {
			_city
		}
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_new_schema_query() {
    let input = r#"
	schema {
		pred
		type
	}
	schema( pred : name ) {
		pred
		type
	}
	schema( pred : name,hi ) { #This is valid in lexer, parser would throw error
		pred
		type
	}
	schema( pred : [name,hi] ) {
		pred
		type
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_abrupt_schema_query() {
    let input = r#"
	schema {
		pred
	"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_eq!(items.last().map(|(typ, _)| *typ), Some(ITEM_ERROR));
}

#[test]
fn test_abrupt_mutation() {
    let input = r#"
	mutation {
		set {
			What is <this> .
			Why is this #!!?
			How is this?
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_eq!(items.last().map(|(typ, _)| *typ), Some(ITEM_ERROR));
}

#[test]
fn test_variables1() {
    let input = r#"
	query testQuery($username: String!) {
		me(_xid_: rick) {
			_city
		}
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_variables2() {
    let input = r#"
	query testQuery ($username: String, $id: int, $email: string) {
		me(_xid_: rick) {
			_city
		}
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_variables_default() {
    let input = r#"
	query testQuery ($username: string = abc, $id: int = 5, $email: string) {
		me(_xid_: rick) {
			_city
		}
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_iri_ref_in_identify_mutation_or_query() {
    let input = r#"{
		<user-query>(func: uid(0x1)) {
			name
		}
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_iri_ref() {
    let input = r#"
	query testQuery {
		me(id : <http://helloworld.com/how/are/you>) {
		        <http://verygood.com/what/about/you>
		}
	}"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_lang_support() {
    let input = r#"
	query {
		me(id: test) {
			name@en
		}
	}
	"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_multi_lang_support() {
    let input = r#"
	query {
		me(id: test) {
			name@en, name@en:ru:fr:de
		}
	}
	"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
}

#[test]
fn test_number_in_lang() {
    let input = r#"
	{
		q(func: eq(name@es-419, "aoeu")) {
			name@.
		}
	}
	"#;
    let items = lex(input, LEX_TOP_LEVEL);
    assert_no_error(&items);
    assert!(items.iter().any(|(_, val)| val == "es-419"));
}
