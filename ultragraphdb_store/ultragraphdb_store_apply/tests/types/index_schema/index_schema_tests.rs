/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index_test.go (TestIndexingInt, TestIndexingIntNegative, TestIndexingFloat,
 * TestIndexingTime, TestIndexing, TestIndexingMultiLang, TestIndexingInvalidLang, TestIndexingAliasedLang).
 * Changed for ultragraphdb: translated to Rust; the schema the tokens are built under is the caller's, read from a
 * schema state, where Dgraph reads its global state.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::attr_in_root_namespace;
use ultragraphdb_core_schema::SchemaState;
use ultragraphdb_core_types::{TypeId, Val, Value};
use ultragraphdb_store_apply::IndexSchema;

/// Dgraph's `indexTokensForTest`: the tokens of a string value under the schema `text`.
fn tokens(text: &str, attr: &str, lang: &str, value: &str) -> Vec<Vec<u8>> {
    let state = SchemaState::new();
    state.parse_text(text).unwrap();
    let schema = IndexSchema::from_state(&state, &attr_in_root_namespace(attr)).unwrap();
    schema
        .tokens(
            lang,
            &Val::new(TypeId::STRING, Value::Bytes(value.as_bytes().to_vec())),
        )
        .unwrap()
}

#[test]
fn test_indexing_int() {
    assert_eq!(
        tokens("age:int @index(int) .", "age", "", "10")[0],
        [0x6, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xa]
    );
}

#[test]
fn test_indexing_int_negative() {
    assert_eq!(
        tokens("age:int @index(int) .", "age", "", "-10")[0],
        [0x6, 0x0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf6]
    );
}

#[test]
fn test_indexing_float() {
    assert_eq!(
        tokens("age:float @index(float) .", "age", "", "10.43")[0],
        [0x7, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xa]
    );
}

#[test]
fn test_indexing_time() {
    assert_eq!(
        tokens(
            "age:dateTime @index(year) .",
            "age",
            "",
            "0010-01-01T01:01:01.000000001"
        )[0],
        [0x4, 0x0, 0xa]
    );
}

#[test]
fn test_indexing() {
    assert_eq!(
        tokens("name:string @index(term) .", "name", "", "abc")[0],
        b"\x01abc"
    );
}

#[test]
fn test_indexing_multi_lang() {
    let schema = "name:string @index(fulltext) .";
    assert_eq!(tokens(schema, "name", "", "stemming")[0], b"\x08stem");
    assert_eq!(
        tokens(schema, "name", "fi", "edeltäneessä")[0],
        "\x08edeltän".as_bytes()
    );
    assert_eq!(
        tokens(schema, "name", "de", "Auffassungsvermögen")[0],
        b"\x08auffassungsvermog"
    );
    assert_eq!(
        tokens(schema, "name", "", "Auffassungsvermögen")[0],
        "\x08auffassungsvermögen".as_bytes()
    );
}

#[test]
fn test_indexing_invalid_lang() {
    // The tokenizer for language xx returns no error.
    tokens("name:string @index(fulltext) .", "name", "xx", "error");
}

#[test]
fn test_indexing_aliased_lang() {
    let schema = "name:string @index(fulltext) @lang .";
    tokens(schema, "name", "es", "base");
    // es-es and es-419 are aliased to es.
    tokens(schema, "name", "es-es", "alias");
    tokens(schema, "name", "es-419", "alias");
}

/// Dgraph's `indexTokens` refusals: a value of a predicate without a scalar type, and one without an index.
#[test]
fn test_a_value_without_a_scalar_type_or_an_index_has_no_tokens() {
    let state = SchemaState::new();
    state.parse_text("friend: [uid] .\nname: string .").unwrap();
    let v = Val::new(TypeId::STRING, Value::Bytes(b"x".to_vec()));
    let err = IndexSchema::from_state(&state, &attr_in_root_namespace("friend"))
        .unwrap()
        .tokens("", &v)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Cannot index attribute 0-friend of type object."
    );
    let err = IndexSchema::from_state(&state, &attr_in_root_namespace("name"))
        .unwrap()
        .tokens("", &v)
        .unwrap_err();
    assert_eq!(err.to_string(), "Attribute 0-name is not indexed.");
}
