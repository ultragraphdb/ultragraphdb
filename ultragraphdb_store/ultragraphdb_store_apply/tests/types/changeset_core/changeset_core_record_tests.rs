/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The rebuild record across a crash: a partition reopened with a record loads its catalog from the metadata group's
//! version, drops every index the record names that the version does not declare, and builds again those it declares,
//! so the version's indexes are complete and nothing else remains, wherever the build was interrupted.

use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::{Applied, ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_sim::{Fault, RebuildStage};

use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    NS, TestChangesets, changesets, changesets_over, declaration, declare, declared, exact_token,
    finish_builds, index_keys_holding, indexed, is_held_in_default, keys_of, term_token, uid,
    write,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The applier of `a`'s store after a crash and a restart, which loads `name` declared as `phrase` in `namespace`, the
/// metadata group's version in effect, and recovers its builds, run to their end.
fn restarted(a: TestChangesets, namespace: u64, phrase: &str) -> TestResult<TestChangesets> {
    let store = a.quads().store().clone();
    store.crash();
    store.recover();
    let restarted = changesets_over(store)?;
    // The applier the crash ended goes once the restarted one is open, as it always did.
    drop(a);
    let name = Declaration::Predicate(declaration("name", phrase)?);
    restarted.load_catalog(&CatalogContents::new(namespace, 1, vec![name]))?;
    restarted.recover_builds()?;
    finish_builds(&restarted);
    Ok(restarted)
}

/// `name` indexed by `term` holding `"big apple"` for `a`; `exact` added while `b` writes `"red pear"`, the build
/// interrupted at `stage`, or never run without one; the store restarted.
///
/// With the store's refusal of the declaration, when the crash stopped the round that declares it.
fn interrupted(
    stage: Option<RebuildStage>,
) -> TestResult<(TestChangesets, Option<ChangesetError>)> {
    let a = changesets()?;
    declare(&a, "name", "with `term`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    if let Some(stage) = stage {
        a.quads()
            .store()
            .inject(Fault::CrashDuringNextRebuild(stage));
    }
    let refusal = match declared(&a, "name", "with `term` and `exact`")? {
        Ok(_) => {
            assert_eq!(a.quads().store().rebuild_records()?.len(), 1);
            write(&a, &["<b> <name> \"red pear\""])?;
            if stage.is_some() {
                finish_builds(&a);
            }
            None
        }
        Err(e) => Some(e),
    };
    // The version in effect is the store's: the new declaration once its round committed, the old one otherwise.
    let loaded = if refusal.is_none() {
        "with `term` and `exact`"
    } else {
        "with `term`"
    };
    Ok((restarted(a, NS, loaded)?, refusal))
}

/// The catalog declares `term`, and `exact` with it or not; every index it declares holds each literal held, and no
/// other index key remains; no record remains.
fn assert_complete(a: &TestChangesets, case: &str) -> TestResult {
    let declared = a.catalog(NS)?.predicate(uid(a, "name")?).cloned();
    let with_exact = declared == Some(declaration("name", "with `term` and `exact`")?);
    assert!(
        with_exact || declared == Some(declaration("name", "with `term`")?),
        "{case}: {declared:?}"
    );
    let mut expected_keys = 0;
    for (subject, words, text) in [
        ("a", ["big", "apple"], "big apple"),
        ("b", ["red", "pear"], "red pear"),
    ] {
        if !is_held_in_default(a, subject, "name")? {
            continue;
        }
        let node = uid(a, subject)?;
        for word in words {
            assert_eq!(
                indexed(a, "name", &term_token(word))?,
                vec![node],
                "{case}: {word}"
            );
        }
        assert_eq!(
            indexed(a, "name", &exact_token(text))? == vec![node],
            with_exact,
            "{case}: {text}"
        );
        expected_keys += if with_exact { 3 } else { 2 };
        assert_eq!(
            index_keys_holding(a, "name", node)?.len(),
            if with_exact { 3 } else { 2 },
            "{case}"
        );
    }
    assert_eq!(
        keys_of(a, Keyspace::ValueIndex, "name")?.len(),
        expected_keys,
        "{case}"
    );
    if !with_exact {
        assert!(
            keys_of(a, Keyspace::ValueIndex, "name")?
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
    assert!(a.quads().store().rebuild_records()?.is_empty(), "{case}");
    assert!(!a.rebuilding(), "{case}");
    Ok(())
}

#[test]
fn test_a_build_never_run_is_built_again_at_open() -> TestResult {
    let (a, refusal) = interrupted(None)?;
    assert_eq!(refusal, None);
    assert!(is_held_in_default(&a, "b", "name")?);
    // The version's declaration, its indexes built again from the data.
    assert_eq!(
        a.catalog(NS)?.predicate(uid(&a, "name")?),
        Some(&declaration("name", "with `term` and `exact`")?)
    );
    assert_complete(&a, "never run")?;
    Ok(())
}

#[test]
fn test_a_build_interrupted_at_any_stage_is_built_again_at_open() -> TestResult {
    for stage in RebuildStage::ALL {
        let (a, refusal) = interrupted(Some(stage))?;
        // The record is written by the declaring round, before its batch: a crash at the record's first three stages
        // stops that round, which is refused with the simulated store's crash text, and the build never starts. A crash
        // at a temporary batch stops the build itself, after the round committed; one at the record's removal stops
        // it after the swap's batch was written, which holds the new declaration.
        let refused_round = matches!(
            stage,
            RebuildStage::Start | RebuildStage::RecordWritten | RebuildStage::RecordSynced
        );
        match &refusal {
            Some(e) => assert!(
                refused_round
                    && e.0
                        == ChangesetErrorEnum::Store(StoreError::write_failed(
                            "simulated process crash during a rebuild".into()
                        )),
                "{stage:?}: {e:?}"
            ),
            None => assert!(!refused_round, "{stage:?}: the declaration applied"),
        }
        assert_eq!(
            is_held_in_default(&a, "b", "name")?,
            !refused_round,
            "{stage:?}: `b` written after the declaration"
        );
        let expected = if refused_round {
            declaration("name", "with `term`")?
        } else {
            declaration("name", "with `term` and `exact`")?
        };
        assert_eq!(
            a.catalog(NS)?.predicate(uid(&a, "name")?),
            Some(&expected),
            "{stage:?}"
        );
        assert_complete(&a, &format!("{stage:?}"))?;
        // The store serves writes after the restart, and maintains what it declares.
        write(&a, &["<c> <name> \"green fig\""])?;
        assert_eq!(
            indexed(&a, "name", &term_token("fig"))?,
            vec![uid(&a, "c")?],
            "{stage:?}"
        );
    }
    Ok(())
}

#[test]
fn test_an_index_removal_leaves_no_record() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `term` and `exact`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    assert!(!a.rebuilding());
    assert!(a.quads().store().rebuild_records()?.is_empty());
    assert!(indexed(&a, "name", &term_token("big"))?.is_empty());
    assert_eq!(
        indexed(&a, "name", &exact_token("big apple"))?,
        vec![uid(&a, "a")?]
    );
    Ok(())
}

#[test]
fn test_a_record_of_another_namespace_is_finished_against_that_namespace_s_catalog() -> TestResult {
    use ultragraphdb_core_mapping::{Change, Changeset};
    let a = changesets()?;
    let three = |changes: Vec<Change>| changes.into_iter().fold(Changeset::new(3), Changeset::with);
    let declare_three = |phrase: &str| -> TestResult<Change> {
        Ok(Change::Declare(Declaration::Predicate(declaration(
            "name", phrase,
        )?)))
    };
    let _applied: Applied = a.apply(&three(vec![declare_three("with `term`")?]))?;
    let _written: Applied = a.apply(&three(vec![crate::utils::changesets::insert(
        "<a> <name> \"big apple\"",
    )?]))?;
    let _redeclared: Applied = a.apply(&three(vec![declare_three("with `term` and `exact`")?]))?;
    assert!(a.rebuilding());
    let a = restarted(a, 3, "with `term` and `exact`")?;
    let name = a
        .quads()
        .id_of(3, "http://ex.org/name")?
        .context("<name> has a UID in namespace 3")?;
    assert_eq!(
        a.catalog(3)?.predicate(name),
        Some(&declaration("name", "with `term` and `exact`")?)
    );
    // The version's indexes, built again: the `term` keys "big" and "apple" and the `exact` key "big apple".
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
    assert_eq!(tokenizers, vec![0x01, 0x01, 0x02]);
    assert!(a.quads().store().rebuild_records()?.is_empty());
    Ok(())
}
