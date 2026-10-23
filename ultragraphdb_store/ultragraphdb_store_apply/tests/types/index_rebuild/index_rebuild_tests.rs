/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index_test.go (TestNeedsTokIndexRebuild, TestNeedsCountIndexRebuild,
 * TestNeedsReverseEdgesRebuild, TestNeedsListTypeRebuild). Changed for ultragraphdb: translated to Rust; an absent
 * list of tokenizers is an empty one; added: the interim query schema and the prefixes each decision drops.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{BYTE_SPLIT, attr_in_root_namespace, predicate_prefix};
use ultragraphdb_core_tok::{IDENT_EXACT, IDENT_EXACT_LANG, IDENT_TERM};
use ultragraphdb_proto_pb::pb::schema_update::Directive;
use ultragraphdb_proto_pb::pb::{SchemaUpdate, posting::ValType};
use ultragraphdb_store_apply::{IndexOp, IndexRebuild, prefixes_to_delete_tokens_for};

fn su(value_type: ValType, directive: Directive, tokenizer: &[&str]) -> SchemaUpdate {
    SchemaUpdate {
        value_type: value_type as i32,
        directive: directive as i32,
        tokenizer: tokenizer.iter().map(|t| t.to_string()).collect(),
        ..SchemaUpdate::default()
    }
}

fn rb(old: Option<SchemaUpdate>, current: SchemaUpdate) -> IndexRebuild {
    IndexRebuild::new("", 0, old, current)
}

#[test]
fn test_needs_tok_index_rebuild() {
    let uid = su(ValType::Uid, Directive::None, &[]);
    let info = rb(Some(uid.clone()), uid.clone())
        .needs_tok_index_rebuild()
        .unwrap();
    assert_eq!(info.op(), IndexOp::Noop);
    assert!(info.tokenizers_to_delete().is_empty());
    assert!(info.tokenizers_to_rebuild().is_empty());

    let info = rb(None, uid).needs_tok_index_rebuild().unwrap();
    assert_eq!(info.op(), IndexOp::Noop);
    assert!(info.tokenizers_to_delete().is_empty());
    assert!(info.tokenizers_to_rebuild().is_empty());

    let exact = su(ValType::String, Directive::Index, &["exact"]);
    let info = rb(Some(exact.clone()), exact.clone())
        .needs_tok_index_rebuild()
        .unwrap();
    assert_eq!(info.op(), IndexOp::Noop);
    assert!(info.tokenizers_to_delete().is_empty());
    assert!(info.tokenizers_to_rebuild().is_empty());

    let term = su(ValType::String, Directive::Index, &["term"]);
    let none = su(ValType::String, Directive::Index, &[]);
    let info = rb(Some(term), none).needs_tok_index_rebuild().unwrap();
    assert_eq!(info.op(), IndexOp::Rebuild);
    assert_eq!(info.tokenizers_to_delete(), ["term"]);
    assert!(info.tokenizers_to_rebuild().is_empty());

    let float_exact = su(ValType::Float, Directive::Index, &["exact"]);
    let info = rb(Some(exact.clone()), float_exact)
        .needs_tok_index_rebuild()
        .unwrap();
    assert_eq!(info.op(), IndexOp::Rebuild);
    assert_eq!(info.tokenizers_to_delete(), ["exact"]);
    assert_eq!(info.tokenizers_to_rebuild(), ["exact"]);

    let float_none = su(ValType::Float, Directive::None, &[]);
    let info = rb(Some(exact), float_none)
        .needs_tok_index_rebuild()
        .unwrap();
    assert_eq!(info.op(), IndexOp::Delete);
    assert_eq!(info.tokenizers_to_delete(), ["exact"]);
    assert!(info.tokenizers_to_rebuild().is_empty());
}

#[test]
fn test_needs_count_index_rebuild() {
    let count = |c: bool| SchemaUpdate {
        count: c,
        ..su(ValType::Uid, Directive::None, &[])
    };
    assert_eq!(
        rb(Some(count(false)), count(true)).needs_count_index_rebuild(),
        IndexOp::Rebuild
    );
    assert_eq!(
        rb(None, count(true)).needs_count_index_rebuild(),
        IndexOp::Rebuild
    );
    assert_eq!(
        rb(Some(count(false)), count(false)).needs_count_index_rebuild(),
        IndexOp::Noop
    );
    assert_eq!(
        rb(Some(count(true)), count(false)).needs_count_index_rebuild(),
        IndexOp::Delete
    );
}

#[test]
fn test_needs_reverse_edges_rebuild() {
    let index = su(ValType::Uid, Directive::Index, &[]);
    let reverse = su(ValType::Uid, Directive::Reverse, &[]);
    assert_eq!(
        rb(Some(index.clone()), reverse.clone()).needs_reverse_edges_rebuild(),
        IndexOp::Rebuild
    );
    assert_eq!(
        rb(None, reverse.clone()).needs_reverse_edges_rebuild(),
        IndexOp::Rebuild
    );
    assert_eq!(
        rb(Some(reverse.clone()), reverse.clone()).needs_reverse_edges_rebuild(),
        IndexOp::Noop
    );
    assert_eq!(
        rb(Some(reverse), index).needs_reverse_edges_rebuild(),
        IndexOp::Delete
    );
}

#[test]
fn test_needs_list_type_rebuild() {
    let list = |l: bool| SchemaUpdate {
        list: l,
        ..su(ValType::Uid, Directive::None, &[])
    };
    assert_eq!(
        rb(Some(list(false)), list(true)).needs_list_type_rebuild(),
        Ok(true)
    );
    assert_eq!(rb(None, list(true)).needs_list_type_rebuild(), Ok(false));
    let scalar = SchemaUpdate {
        predicate: attr_in_root_namespace(""),
        ..list(false)
    };
    let err = rb(Some(list(true)), scalar)
        .needs_list_type_rebuild()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Type can't be changed from list to scalar for attr: [] without dropping it first."
    );
}

/// The interim schema queries use while indexes are built: the new schema with the old tokenizers less those it
/// drops, and without the count index or reverse edges being built (`index.go:1132-1160`).
#[test]
fn test_the_query_schema_holds_neither_the_index_built_nor_the_one_removed() {
    let term = su(ValType::String, Directive::Index, &["term"]);
    let exact = su(ValType::String, Directive::Index, &["exact"]);
    let query = rb(Some(term.clone()), exact.clone())
        .get_query_schema()
        .unwrap();
    assert!(query.tokenizer.is_empty(), "{query:?}");
    let both = su(ValType::String, Directive::Index, &["term", "exact"]);
    let query = rb(Some(both), exact).get_query_schema().unwrap();
    assert_eq!(query.tokenizer, ["exact"]);
    let counted = SchemaUpdate {
        count: true,
        ..su(ValType::Uid, Directive::Reverse, &[])
    };
    let query = rb(Some(su(ValType::Uid, Directive::None, &[])), counted)
        .get_query_schema()
        .unwrap();
    assert!(!query.count);
    assert_eq!(query.directive, Directive::None as i32);
    assert!(rb(None, term).need_index_rebuild().unwrap());
}

/// Dgraph's `prefixesToDeleteTokensFor`: the tokenizer's index prefix and its split twin, the exact tokenizer's
/// language identifier with them.
#[test]
fn test_the_prefixes_of_a_tokenizer_pair_each_with_its_split_twin() {
    let attr = attr_in_root_namespace("name");
    let mut index = predicate_prefix(&attr).unwrap();
    index.push(ultragraphdb_core_keys::BYTE_INDEX);
    let with = |first: u8, id: u8| {
        let mut p = index.clone();
        p[0] = first;
        p.push(id);
        p
    };
    assert_eq!(
        prefixes_to_delete_tokens_for(&attr, "term", false).unwrap(),
        vec![with(0, IDENT_TERM), with(BYTE_SPLIT, IDENT_TERM)]
    );
    assert_eq!(
        prefixes_to_delete_tokens_for(&attr, "exact", true).unwrap(),
        vec![
            with(0, IDENT_EXACT_LANG),
            with(BYTE_SPLIT, IDENT_EXACT_LANG)
        ]
    );
    assert_eq!(
        prefixes_to_delete_tokens_for(&attr, "nosuch", false)
            .unwrap_err()
            .to_string(),
        "Could not find valid tokenizer for nosuch"
    );
    let info = IndexRebuild::new(
        &attr,
        0,
        Some(su(ValType::String, Directive::Index, &["exact"])),
        su(ValType::String, Directive::None, &[]),
    )
    .needs_tok_index_rebuild()
    .unwrap();
    assert_eq!(
        info.prefixes_for_tok_indexes().unwrap(),
        vec![
            with(0, IDENT_EXACT),
            with(BYTE_SPLIT, IDENT_EXACT),
            with(0, IDENT_EXACT_LANG),
            with(BYTE_SPLIT, IDENT_EXACT_LANG)
        ]
    );
}
