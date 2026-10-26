/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The rebuild record across a crash: a store reopened with a record keeps the declaration its catalog holds, the
//! interim one or the new one, with that declaration's indexes complete, and drops every other index the record
//! names, wherever the build was interrupted.

use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_store_sim::{Fault, RebuildStage};

use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    NS, TestChangesets, changesets, changesets_over, declaration, declare, declared, exact_token,
    finish_builds, index_keys_holding, indexed, is_held_in_default, keys_of, term_token, uid,
    write,
};

/// The applier of `a`'s store after a crash and a restart.
fn restarted(a: TestChangesets) -> TestChangesets {
    let store = a.quads().store().clone();
    store.crash();
    store.recover();
    changesets_over(store)
}

/// `name` indexed by `term` holding `"big apple"` for `a`; `exact` added while `b` writes `"red pear"`, the build
/// interrupted at `stage`, or never run without one; the store restarted.
fn interrupted(stage: Option<RebuildStage>) -> TestChangesets {
    let a = changesets();
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    if let Some(stage) = stage {
        a.quads()
            .store()
            .inject(Fault::CrashDuringNextRebuild(stage));
    }
    if declared(&a, "name", "with `term` and `exact`").is_ok() {
        assert_eq!(a.quads().store().rebuild_records().unwrap().len(), 1);
        write(&a, &["<b> <name> \"red pear\""]);
        if stage.is_some() {
            finish_builds(&a);
        }
    }
    restarted(a)
}

/// The catalog declares `term`, and `exact` with it or not; every index it declares holds each literal held, and no
/// other index key remains; no record remains.
fn assert_complete(a: &TestChangesets, case: &str) {
    let declared = a.catalog(NS).unwrap().predicate(uid(a, "name")).cloned();
    let with_exact = declared == Some(declaration("name", "with `term` and `exact`"));
    assert!(
        with_exact || declared == Some(declaration("name", "with `term`")),
        "{case}: {declared:?}"
    );
    let mut expected_keys = 0;
    for (subject, words, text) in [
        ("a", ["big", "apple"], "big apple"),
        ("b", ["red", "pear"], "red pear"),
    ] {
        if !is_held_in_default(a, subject, "name") {
            continue;
        }
        let node = uid(a, subject);
        for word in words {
            assert_eq!(
                indexed(a, "name", &term_token(word)),
                vec![node],
                "{case}: {word}"
            );
        }
        assert_eq!(
            indexed(a, "name", &exact_token(text)) == vec![node],
            with_exact,
            "{case}: {text}"
        );
        expected_keys += if with_exact { 3 } else { 2 };
        assert_eq!(
            index_keys_holding(a, "name", node).len(),
            if with_exact { 3 } else { 2 },
            "{case}"
        );
    }
    assert_eq!(
        keys_of(a, Keyspace::ValueIndex, "name").len(),
        expected_keys,
        "{case}"
    );
    if !with_exact {
        assert!(
            keys_of(a, Keyspace::ValueIndex, "name")
                .iter()
                .all(|k| !matches!(
                    QuadKey::parse(k),
                    Ok((
                        QuadKey::ValueIndex {
                            tokenizer: 0x02,
                            ..
                        },
                        _
                    ))
                )),
            "{case}: no exact key remains"
        );
    }
    assert!(
        a.quads().store().rebuild_records().unwrap().is_empty(),
        "{case}"
    );
    assert!(!a.rebuilding(), "{case}");
}

#[test]
fn test_a_build_never_run_is_finished_at_open_with_the_interim_declaration() {
    let a = interrupted(None);
    assert!(is_held_in_default(&a, "b", "name"));
    // The interim declaration, the new one without `exact`, which was being built.
    assert_eq!(
        a.catalog(NS).unwrap().predicate(uid(&a, "name")),
        Some(&declaration("name", "with `term`"))
    );
    assert_complete(&a, "never run");
}

#[test]
fn test_a_build_interrupted_at_any_stage_is_finished_at_open() {
    for stage in RebuildStage::ALL {
        let a = interrupted(Some(stage));
        assert_complete(&a, &format!("{stage:?}"));
        // The store serves writes after the restart, and maintains what it declares.
        write(&a, &["<c> <name> \"green fig\""]);
        assert_eq!(
            indexed(&a, "name", &term_token("fig")),
            vec![uid(&a, "c")],
            "{stage:?}"
        );
    }
}

#[test]
fn test_an_index_removal_leaves_no_record() {
    let a = changesets();
    declare(&a, "name", "with `term` and `exact`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    assert!(!a.rebuilding());
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
    assert!(indexed(&a, "name", &term_token("big")).is_empty());
    assert_eq!(
        indexed(&a, "name", &exact_token("big apple")),
        vec![uid(&a, "a")]
    );
}

#[test]
fn test_a_record_of_another_namespace_is_finished_against_that_namespace_s_catalog() {
    use ultragraphdb_core_mapping::{Change, Changeset, Declaration};
    let a = changesets();
    let three = |changes: Vec<Change>| changes.into_iter().fold(Changeset::new(3), Changeset::with);
    let declare_three =
        |phrase: &str| Change::Declare(Declaration::Predicate(declaration("name", phrase)));
    a.apply(&three(vec![declare_three("with `term`")])).unwrap();
    a.apply(&three(vec![crate::utils::changesets::insert(
        "<a> <name> \"big apple\"",
    )]))
    .unwrap();
    a.apply(&three(vec![declare_three("with `term` and `exact`")]))
        .unwrap();
    assert!(a.rebuilding());
    let a = restarted(a);
    let name = a.quads().uid_of(3, "http://ex.org/name").unwrap().unwrap();
    assert_eq!(
        a.catalog(3).unwrap().predicate(name),
        Some(&declaration("name", "with `term`"))
    );
    // The interim declaration's `term` keys are kept: "big" and "apple", nothing else.
    let tokenizers: Vec<u8> = crate::utils::changesets::stored_keys(&a)
        .iter()
        .filter_map(|k| match QuadKey::parse(k) {
            Ok((
                QuadKey::ValueIndex {
                    namespace: 3,
                    tokenizer,
                    ..
                },
                _,
            )) => Some(tokenizer),
            _ => None,
        })
        .collect();
    assert_eq!(tokenizers, vec![0x01, 0x01]);
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
}
