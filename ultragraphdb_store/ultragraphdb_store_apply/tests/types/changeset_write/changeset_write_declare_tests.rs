/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Declarations and the decision table: an index added to a predicate holding data is built, one added to a predicate
//! without data is declared at once, one removed has its keys deleted without a build; one build runs at a time; a
//! vector index's options are checked by its factory, and its dimension is stated; entity types and relations are
//! declared with their entries.

use std::num::NonZeroU16;

use ultragraphdb_core_catalog::CatalogError;
use ultragraphdb_core_keys::{CountDirection, Keyspace};
use ultragraphdb_core_mapping::{
    Change, Changeset, Declaration, PredicateDeclaration, RelationDeclaration, TypeDeclaration,
    VectorIndexSpec,
};
use ultragraphdb_store_apply::{Applied, ChangesetErrorEnum};

use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    NS, apply, changeset, changesets, counts_of, declaration, declare, declared, ex, finish_builds,
    keys_of, reverse, run_indexer, uid, write, write_changes,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_an_index_declared_on_a_predicate_without_data_needs_no_build() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `exact`")?;
    assert!(!a.rebuilding());
    assert_eq!(
        a.catalog(NS)?.predicate(uid(&a, "name")?),
        Some(&declaration("name", "with `exact`")?)
    );
    assert!(a.quads().store().rebuild_records()?.is_empty());
    Ok(())
}

#[test]
fn test_a_declaration_needing_a_build_while_one_runs_is_refused() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <name> \"Ann\"", "<a> <tag> \"x\""])?;
    declare(&a, "name", "with `exact`")?;
    assert!(a.rebuilding());
    for (predicate, phrase) in [
        ("tag", "with `exact`"),
        ("name", "with `term`"),
        ("name", "single-valued"),
    ] {
        let refused = declared(&a, predicate, phrase)?
            .err()
            .context("the error of declared(&a, predicate, phrase)?")?;
        assert!(
            matches!(refused.0, ChangesetErrorEnum::Rebuilding { .. }),
            "{refused:?}"
        );
        assert_eq!(
            refused.to_string(),
            format!(
                "An index is being built, so <http://ex.org/{predicate}> cannot be declared now. Please retry"
            )
        );
    }
    // A declaration of another predicate that builds nothing is applied.
    declare(&a, "tag", "single-valued")?;
    finish_builds(&a);
    assert!(!a.rebuilding());
    declare(&a, "tag", "single-valued with `exact`")?;
    assert!(a.rebuilding());
    finish_builds(&a);
    assert_eq!(
        a.catalog(NS)?.predicate(uid(&a, "tag")?),
        Some(&declaration("tag", "single-valued with `exact`")?)
    );
    Ok(())
}

#[test]
fn test_two_builds_executed_together_refuse_the_second() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <name> \"Ann\"", "<a> <tag> \"x\""])?;
    let declare_change = |p: &str| -> TestResult<Changeset> {
        Ok(changeset(vec![Change::Declare(Declaration::Predicate(
            declaration(p, "with `exact`")?,
        ))]))
    };
    let (name, tag) = (
        a.execute(&declare_change("name")?)?,
        a.execute(&declare_change("tag")?)?,
    );
    let next = a.quads().lists().next_version();
    let _applied: Vec<Applied> = a.apply_round(vec![(next, name)])?;
    let refused = a
        .apply_round(vec![(next + 2, tag)])
        .err()
        .context("the error of a.apply_round(vec![(next + 2, tag)])")?;
    assert_eq!(
        refused.to_string(),
        "An index is being built, so <http://ex.org/tag> cannot be declared now. Please retry"
    );
    finish_builds(&a);
    // `tag` keeps the default entry its first write recorded.
    assert_eq!(
        a.catalog(NS)?.predicate(uid(&a, "tag")?),
        Some(&PredicateDeclaration::new(ex("tag")))
    );
    Ok(())
}

#[test]
fn test_removing_the_count_index_deletes_its_keys_and_keeps_the_reverse_lists() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    write(&a, &["<a> <friend> <b>", "<a> <friend> <c>"])?;
    assert!(!keys_of(&a, Keyspace::CountIndex, "friend")?.is_empty());
    declare(&a, "friend", "")?;
    assert!(!a.rebuilding(), "removing an index builds nothing");
    assert!(keys_of(&a, Keyspace::CountIndex, "friend")?.is_empty());
    assert_eq!(reverse(&a, "friend", "b")?, vec![uid(&a, "a")?]);
    assert!(a.quads().store().rebuild_records()?.is_empty());
    Ok(())
}

#[test]
fn test_removing_the_vector_index_deletes_its_lists_and_keeps_the_vectors() -> TestResult {
    let a = changesets()?;
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    )?;
    write(
        &a,
        &[
            "<a> <emb> \"[1, 0]\"^^ugdb:float32vector",
            "<b> <emb> \"[0, 1]\"^^ugdb:float32vector",
        ],
    )?;
    run_indexer(&a, "emb")?;
    assert!(!keys_of(&a, Keyspace::VectorIndex, "emb")?.is_empty());
    declare(&a, "emb", "single-valued")?;
    assert!(!a.rebuilding());
    assert!(keys_of(&a, Keyspace::VectorIndex, "emb")?.is_empty());
    assert_eq!(keys_of(&a, Keyspace::Forward, "emb")?.len(), 2);
    Ok(())
}

#[test]
fn test_a_vector_index_without_a_dimension_is_refused_and_not_held() -> TestResult {
    let a = changesets()?;
    let refused = declared(&a, "emb", "single-valued with a vector index")?
        .err()
        .context("the error of declared(..)")?;
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::NoDimension { predicate } if predicate == ex("emb").as_str()),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "Predicate <http://ex.org/emb> declares a vector index without its dimension"
    );
    // Nothing of the declaration is held: no catalog entry for `emb`.
    let held = match a.quads().id_of(NS, ex("emb").as_str())? {
        Some(p) => a.catalog(NS)?.predicate(p).cloned(),
        None => None,
    };
    assert_eq!(held, None);
    Ok(())
}

#[test]
fn test_a_vector_index_s_dimension_is_held_in_the_catalog() -> TestResult {
    let a = changesets()?;
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 768",
    )?;
    let declared = a.catalog(NS)?.predicate(uid(&a, "emb")?).cloned();
    assert_eq!(
        declared
            .as_ref()
            .and_then(|d| d.vector_index())
            .and_then(|v| v.dimension()),
        NonZeroU16::new(768)
    );
    // The catalog loaded again from the store holds it too.
    a.forget_catalogs();
    assert_eq!(a.catalog(NS)?.predicate(uid(&a, "emb")?).cloned(), declared);
    Ok(())
}

#[test]
fn test_vector_options_their_factory_refuses_are_refused() -> TestResult {
    let a = changesets()?;
    let two = NonZeroU16::new(2).context("2 is not zero")?;
    let with = |options: Vec<(String, String)>| {
        PredicateDeclaration::new(ex("emb"))
            .with_vector_index(VectorIndexSpec::new("hnsw", options).with_dimension(two))
    };
    let refused = apply(
        &a,
        vec![Change::Declare(Declaration::Predicate(with(vec![(
            "maxLevels".into(),
            "x".into(),
        )])))],
    )
    .err()
    .context("the error of apply(..)")?;
    // maxLevels takes an integer, parsed by Go's strconv.Atoi (Dgraph's `IntOptParser`, tok/options/option_parser.go),
    // whose error for "x" is `strconv.Atoi: parsing "x": invalid syntax`.
    assert_eq!(
        refused.0,
        ChangesetErrorEnum::VectorOption {
            factory: "hnsw".into(),
            error: "strconv.Atoi: parsing \"x\": invalid syntax".into(),
        }
    );
    assert_eq!(
        refused.to_string(),
        "IndexFactory: hnsw issues this error: 'strconv.Atoi: parsing \"x\": invalid syntax'"
    );
    // A factory the registry does not know is the catalog's refusal, checked before any option or dimension.
    let unknown = PredicateDeclaration::new(ex("emb"))
        .with_vector_index(VectorIndexSpec::new("nope", Vec::new()));
    let unknown_refused = apply(&a, vec![Change::Declare(Declaration::Predicate(unknown))])
        .err()
        .context("the error of apply(..)")?;
    assert_eq!(
        unknown_refused.0,
        ChangesetErrorEnum::Catalog(CatalogError::unknown_vector_index(
            ex("emb").as_str(),
            "nope"
        ))
    );
    assert_eq!(
        unknown_refused.to_string(),
        "Predicate <http://ex.org/emb> declares the unknown vector index nope"
    );
    // Neither refusal declared `emb`.
    let held = match a.quads().id_of(NS, ex("emb").as_str())? {
        Some(p) => a.catalog(NS)?.predicate(p).cloned(),
        None => None,
    };
    assert_eq!(held, None);
    Ok(())
}

#[test]
fn test_entity_types_and_relations_are_declared_with_their_entries() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![
            Change::Declare(Declaration::EntityType(TypeDeclaration::new(
                ex("Agent"),
                None,
                true,
            ))),
            Change::Declare(Declaration::EntityType(TypeDeclaration::new(
                ex("Person"),
                Some(ex("Agent")),
                false,
            ))),
            Change::Declare(Declaration::Relation(RelationDeclaration::new(
                ex("Marriage"),
                vec![ex("spouse1"), ex("spouse2")],
            ))),
        ],
    )?;
    a.forget_catalogs();
    let catalog = a.catalog(NS)?;
    let person = catalog
        .entity_type(uid(&a, "Person")?)
        .context("Person is declared")?;
    assert_eq!(person.parent(), Some(&ex("Agent")));
    assert!(!person.is_abstract());
    assert!(
        catalog
            .entity_type(uid(&a, "Agent")?)
            .is_some_and(TypeDeclaration::is_abstract)
    );
    assert_eq!(
        catalog
            .relation(uid(&a, "Marriage")?)
            .map(RelationDeclaration::roles),
        Some(&[ex("spouse1"), ex("spouse2")][..])
    );
    Ok(())
}

#[test]
fn test_removing_the_exact_index_deletes_the_keys_of_its_language_tokenizer() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `exact`")?;
    write(&a, &["<a> <name> \"Ann\"@en", "<b> <name> \"Bob\""])?;
    // A tagged literal's exact token is under the exact tokenizer with a language, 0x03 (`tok/tok.go`).
    let tokenizers: Vec<u8> = keys_of(&a, Keyspace::ValueIndex, "name")?
        .iter()
        .filter_map(|k| match ultragraphdb_core_keys::QuadKey::parse(k) {
            Ok((ultragraphdb_core_keys::QuadKey::ValueIndex { tokenizer, .. }, _)) => {
                Some(tokenizer)
            }
            _ => None,
        })
        .collect();
    assert!(
        tokenizers.contains(&0x02) && tokenizers.contains(&0x03),
        "{tokenizers:?}"
    );
    declare(&a, "name", "")?;
    assert!(keys_of(&a, Keyspace::ValueIndex, "name")?.is_empty());
    Ok(())
}

#[test]
fn test_an_identical_redeclaration_keeps_the_count_keys() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    write(&a, &["<a> <friend> <b>", "<a> <friend> <c>"])?;
    let before = keys_of(&a, Keyspace::CountIndex, "friend")?;
    // Forward count 2 for `a`, reverse count 1 for `b` and `c`: three keys.
    assert_eq!(before.len(), 3);
    declare(&a, "friend", "with an exact count index")?;
    assert!(!a.rebuilding());
    assert_eq!(keys_of(&a, Keyspace::CountIndex, "friend")?, before);
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, uid(&a, "a")?)?,
        vec![2]
    );
    Ok(())
}
