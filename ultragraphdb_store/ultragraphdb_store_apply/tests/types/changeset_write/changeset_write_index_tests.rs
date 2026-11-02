/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Value-index tokens and count keys as index maintenance writes them, checked against tokens written out by hand:
//! Dgraph's identifiers (`tok/tok.go`: term 0x01, exact 0x02, int 0x06, fulltext 0x08) and its token encodings.

use std::collections::BTreeSet;

use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Keyspace, QuadKey};

use crate::utils::changesets::{
    NS, TestChangesets, changesets, counted, counts_of, declare, delete, delete_pattern,
    exact_token, index_keys_holding, indexed, insert, int_token, keys_of, term_token, uid,
    write_changes,
};
use ultragraphdb_tool_bdd::TestResult;

/// The value-index key of `predicate` for `token`, in the default graph.
fn index_key(
    a: &TestChangesets,
    predicate: &str,
    (tokenizer, token): (u8, Vec<u8>),
) -> TestResult<Vec<u8>> {
    Ok(QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate)?,
        tokenizer,
        token,
    }
    .to_bytes())
}

#[test]
fn test_a_literal_is_indexed_under_exactly_its_tokens() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `term` and `exact`")?;
    write_changes(&a, vec![insert(r#"<a> <name> "Big Apple""#)?])?;
    let held: BTreeSet<Vec<u8>> = index_keys_holding(&a, "name", uid(&a, "a")?)?
        .into_iter()
        .collect();
    let expected: BTreeSet<Vec<u8>> = [
        term_token("big"),
        term_token("apple"),
        exact_token("Big Apple"),
    ]
    .into_iter()
    .map(|t| index_key(&a, "name", t))
    .collect::<TestResult<_>>()?;
    assert_eq!(held, expected);
    Ok(())
}

#[test]
fn test_a_german_literal_is_tokenized_by_the_german_stemmer() -> TestResult {
    let a = changesets()?;
    declare(&a, "body", "with a `fulltext` index")?;
    write_changes(&a, vec![insert(r#"<a> <body> "Häuser"@de"#)?])?;
    // German stemming of "häuser": the suffix "er" off, the umlaut folded, "haus".
    assert_eq!(
        indexed(&a, "body", &(0x08, b"haus".to_vec()))?,
        vec![uid(&a, "a")?]
    );
    assert!(indexed(&a, "body", &(0x08, "häuser".as_bytes().to_vec()))?.is_empty());
    assert_eq!(index_keys_holding(&a, "body", uid(&a, "a")?)?.len(), 1);
    Ok(())
}

#[test]
fn test_integers_of_one_value_share_a_key_and_others_do_not() -> TestResult {
    let a = changesets()?;
    declare(&a, "n", "with an `int` index")?;
    write_changes(
        &a,
        vec![
            insert(r#"<a> <n> "042"^^xsd:integer"#)?,
            insert("<b> <n> 42")?,
            insert("<c> <n> -42")?,
            insert(r#"<d> <n> "9223372036854775807"^^xsd:integer"#)?,
        ],
    )?;
    let (a_, b, c, d) = (uid(&a, "a")?, uid(&a, "b")?, uid(&a, "c")?, uid(&a, "d")?);
    let mut both = vec![a_, b];
    both.sort();
    assert_eq!(indexed(&a, "n", &int_token(42))?, both);
    assert_eq!(indexed(&a, "n", &int_token(-42))?, vec![c]);
    // i64::MAX, the largest value the int index holds.
    assert_eq!(indexed(&a, "n", &int_token(i64::MAX))?, vec![d]);
    Ok(())
}

#[test]
fn test_a_token_shared_by_two_subjects_keeps_the_other() -> TestResult {
    let a = changesets()?;
    declare(&a, "tags", "with a `term` index")?;
    write_changes(
        &a,
        vec![
            insert(r#"<a> <tags> "big apple""#)?,
            insert(r#"<b> <tags> "red apple""#)?,
        ],
    )?;
    write_changes(&a, vec![delete(r#"<a> <tags> "big apple""#)?])?;
    assert_eq!(
        indexed(&a, "tags", &term_token("apple"))?,
        vec![uid(&a, "b")?]
    );
    assert!(indexed(&a, "tags", &term_token("big"))?.is_empty());
    assert_eq!(
        indexed(&a, "tags", &term_token("red"))?,
        vec![uid(&a, "b")?]
    );
    Ok(())
}

#[test]
fn test_each_delete_removes_the_tokens_no_remaining_literal_produces() -> TestResult {
    let a = changesets()?;
    declare(&a, "tags", "with a `term` index")?;
    write_changes(
        &a,
        vec![
            insert(r#"<a> <tags> "big apple""#)?,
            insert(r#"<a> <tags> "red apple""#)?,
            insert(r#"<a> <tags> "red fig""#)?,
        ],
    )?;
    let node = uid(&a, "a")?;
    write_changes(&a, vec![delete(r#"<a> <tags> "red apple""#)?])?;
    for (word, held) in [("big", true), ("apple", true), ("red", true), ("fig", true)] {
        assert_eq!(
            indexed(&a, "tags", &term_token(word))? == vec![node],
            held,
            "{word}"
        );
    }
    write_changes(&a, vec![delete(r#"<a> <tags> "big apple""#)?])?;
    for (word, held) in [
        ("big", false),
        ("apple", false),
        ("red", true),
        ("fig", true),
    ] {
        assert_eq!(
            indexed(&a, "tags", &term_token(word))? == vec![node],
            held,
            "{word}"
        );
    }
    write_changes(&a, vec![delete(r#"<a> <tags> "red fig""#)?])?;
    assert!(index_keys_holding(&a, "tags", node)?.is_empty());
    Ok(())
}

#[test]
fn test_forward_counts_follow_each_insert_and_delete() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    let node = || uid(&a, "a");
    for (object, count) in [("b", 1), ("c", 2), ("d", 3)] {
        write_changes(&a, vec![insert(&format!("<a> <friend> <{object}>"))?])?;
        assert_eq!(
            counts_of(&a, "friend", CountDirection::Forward, node()?)?,
            vec![count]
        );
    }
    write_changes(&a, vec![delete("<a> <friend> <d>")?])?;
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, node()?)?,
        vec![2]
    );
    // Deleting what is not held leaves the count.
    write_changes(&a, vec![delete("<a> <friend> <d>")?])?;
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, node()?)?,
        vec![2]
    );
    write_changes(&a, vec![delete_pattern("a", "friend")])?;
    assert!(counts_of(&a, "friend", CountDirection::Forward, node()?)?.is_empty());
    // A count of zero has no key: none was ever written, in either direction.
    let zero = keys_of(&a, Keyspace::CountIndex, "friend")?
        .into_iter()
        .filter(|k| {
            matches!(
                QuadKey::parse(k),
                Ok((QuadKey::CountIndex { count: 0, .. }, _))
            )
        })
        .count();
    assert_eq!(zero, 0);
    Ok(())
}

#[test]
fn test_equal_counts_in_the_two_directions_stay_apart() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    write_changes(
        &a,
        vec![
            insert("<a> <friend> <b>")?,
            insert("<c> <friend> <b>")?,
            insert("<a> <friend> <c>")?,
        ],
    )?;
    // `a` holds two objects, `b` two subjects: count 2 in each direction, each listing its own node.
    assert_eq!(
        counted(&a, "friend", CountDirection::Forward, 2)?,
        vec![uid(&a, "a")?]
    );
    assert_eq!(
        counted(&a, "friend", CountDirection::Reverse, 2)?,
        vec![uid(&a, "b")?]
    );
    assert_eq!(
        counted(&a, "friend", CountDirection::Forward, 1)?,
        vec![uid(&a, "c")?]
    );
    assert_eq!(
        counted(&a, "friend", CountDirection::Reverse, 1)?,
        vec![uid(&a, "c")?]
    );
    Ok(())
}
