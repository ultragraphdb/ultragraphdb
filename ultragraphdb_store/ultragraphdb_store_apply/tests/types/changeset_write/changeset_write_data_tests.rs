/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Declarations checked against the data: each narrowing the data contradicts is refused naming the predicate, the
//! subject at fault and why, and changes nothing; a widening is applied whatever the data holds.

use ultragraphdb_core_mapping::{Change, Declaration, PredicateKind};
use ultragraphdb_store_apply::{Applied, ChangesetErrorEnum};

use crate::utils::changesets::{
    NS, TestChangesets, apply, changesets, declaration, declare, declared, insert, uid, write,
    write_changes,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn declared_now(
    a: &TestChangesets,
    predicate: &str,
) -> TestResult<Option<ultragraphdb_core_mapping::PredicateDeclaration>> {
    Ok(a.catalog(NS)?.predicate(uid(a, predicate)?).cloned())
}

#[test]
fn test_each_contradicted_narrowing_is_refused_with_its_reason() -> TestResult {
    let cases = [
        (
            vec!["<a> <nick> \"Al\"", "<a> <nick> \"Ally\""],
            "nick",
            "single-valued",
            "The declaration of <http://ex.org/nick> is refused by its data: <http://ex.org/a> holds more than one object",
        ),
        (
            vec!["<a> <name> \"Ann\""],
            "name",
            "an edge",
            "The declaration of <http://ex.org/name> is refused by its data: <http://ex.org/a> holds a literal",
        ),
        (
            vec!["<a> <knows> <b>"],
            "knows",
            "a property",
            "The declaration of <http://ex.org/knows> is refused by its data: <http://ex.org/a> holds an IRI or a blank node",
        ),
        (
            vec!["<a> <knows> _:b"],
            "knows",
            "a property",
            "The declaration of <http://ex.org/knows> is refused by its data: <http://ex.org/a> holds an IRI or a blank node",
        ),
        (
            vec!["<a> <age> 30", "<a> <age> \"old\""],
            "age",
            "with datatype `xsd:integer`",
            "The declaration of <http://ex.org/age> is refused by its data: <http://ex.org/a> holds a literal of another datatype",
        ),
    ];
    for (data, predicate, phrase, text) in cases {
        let a = changesets()?;
        write(&a, &data)?;
        let before = declared_now(&a, predicate)?;
        let refused = declared(&a, predicate, phrase)?
            .err()
            .context("the error of declared(&a, predicate, phrase)?")?;
        assert!(
            matches!(refused.0, ChangesetErrorEnum::Contradicted { .. }),
            "{refused:?}"
        );
        assert_eq!(refused.to_string(), text);
        assert_eq!(
            declared_now(&a, predicate)?,
            before,
            "{predicate} unchanged"
        );
    }
    Ok(())
}

#[test]
fn test_a_narrowing_the_data_meets_is_applied() -> TestResult {
    let a = changesets()?;
    write(
        &a,
        &[
            "<a> <nick> \"Al\"",
            "<b> <nick> \"Bo\"",
            "<a> <knows> <b>",
            "<a> <age> 30",
        ],
    )?;
    // `c`'s list of `nick` is stored and holds no object: its deltas sum to nothing.
    write(&a, &["<c> <nick> \"Cy\""])?;
    write_changes(
        &a,
        vec![crate::utils::changesets::delete("<c> <nick> \"Cy\"")?],
    )?;
    for (predicate, phrase) in [
        ("nick", "single-valued"),
        ("nick", "a property"),
        ("knows", "an edge"),
        ("age", "with datatype `xsd:integer`"),
    ] {
        let _applied: Applied = declared(&a, predicate, phrase)?
            .map_err(|e| format!("{predicate} {phrase}: {e}"))
            .context("the phrase is declared")?;
        assert_eq!(
            declared_now(&a, predicate)?,
            Some(declaration(predicate, phrase)?)
        );
    }
    Ok(())
}

#[test]
fn test_a_widening_is_applied_whatever_the_data_holds() -> TestResult {
    let a = changesets()?;
    declare(&a, "nick", "single-valued")?;
    declare(&a, "age", "with datatype `xsd:integer`")?;
    write(&a, &["<a> <nick> \"Al\"", "<a> <age> 30"])?;
    // Multi-valued, of kind either, and without a datatype constraint.
    for (predicate, phrase) in [("nick", ""), ("age", "")] {
        declare(&a, predicate, phrase)?;
        let d = declared_now(&a, predicate)?.context("declared_now(&a, predicate)?")?;
        assert_eq!(d, declaration(predicate, phrase)?);
        assert_eq!(d.kind(), PredicateKind::Either);
        assert_eq!(d.datatype(), None);
    }
    write(&a, &["<a> <nick> \"Ally\"", "<a> <age> \"old\""])?;
    Ok(())
}

#[test]
fn test_data_in_a_named_graph_or_earlier_in_the_changeset_contradicts() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <name> \"Ann\" <g1>"])?;
    let refused = declared(&a, "name", "an edge")?
        .err()
        .context("the error of declared(&a, \"name\", \"an edge\")?")?;
    assert!(
        refused
            .to_string()
            .ends_with("<http://ex.org/a> holds a literal"),
        "{refused}"
    );
    let b = changesets()?;
    let data_refused = apply(
        &b,
        vec![
            insert("<s> <title> \"T\"")?,
            Change::Declare(Declaration::Predicate(declaration("title", "an edge")?)),
        ],
    )
    .err()
    .context("the error of apply(..)")?;
    assert_eq!(
        data_refused.to_string(),
        "The declaration of <http://ex.org/title> is refused by its data: <http://ex.org/s> holds a literal"
    );
    assert!(
        crate::utils::changesets::holds_nothing(&b, "s")?,
        "<s> holds nothing"
    );
    Ok(())
}

#[test]
fn test_a_declaration_check_reads_the_predicate_s_lists_alone() -> TestResult {
    let a = changesets()?;
    write(&a, &["<t> <title> <x>", "<s> <other> \"y\""])?;
    let applied = apply(
        &a,
        vec![
            insert("<s> <other> \"z\"")?,
            Change::Declare(Declaration::Predicate(declaration("title", "an edge")?)),
        ],
    )?;
    // Hand-counted: the insert reads `s`'s list of `other`, the check `t`'s list of `title`; `s` holds no `title`.
    assert_eq!(applied.lists_read(), 2, "{:?}", applied.read());
    Ok(())
}
