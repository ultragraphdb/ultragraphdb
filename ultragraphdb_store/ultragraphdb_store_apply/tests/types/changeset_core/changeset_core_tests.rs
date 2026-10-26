/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Batches of changesets: each committed or refused on its own, at its own timestamp, a refused one unseen by those
//! after it; a changeset aborted by an earlier one of the batch that assigned the same IRI.

use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_store_quads::QuadErrorEnum;
use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    NS, changeset, changesets, declare, insert, is_held_in_default, objects, term,
};

#[test]
fn test_a_refused_changeset_of_a_batch_leaves_the_others() {
    let a = changesets();
    declare(&a, "knows", "an edge");
    // `a` held before the batch, so no changeset of it assigns `a` a UID.
    a.apply(&changeset(vec![insert("<a> <knows> <z>")]))
        .unwrap();
    let results = a
        .apply_batch(&[
            changeset(vec![insert("<a> <knows> <b>")]),
            changeset(vec![
                insert("<x> <knows> <y>"),
                insert("<x> <knows> \"Bob\""),
            ]),
            changeset(vec![insert("<a> <knows> <c>")]),
        ])
        .unwrap();
    assert_eq!(results.len(), 3);
    let ts: Vec<u64> = [&results[0], &results[2]]
        .iter()
        .map(|r| r.as_ref().expect("commits").commit_ts())
        .collect();
    assert!(ts[0] < ts[1], "{ts:?}");
    assert!(matches!(
        results[1].as_ref().unwrap_err().0,
        ChangesetErrorEnum::Catalog(_)
    ));
    let mut held = objects(&a, "a", "knows");
    held.sort_by_key(|t| t.to_string());
    assert_eq!(held, vec![term("<b>"), term("<c>"), term("<z>")]);
    assert_eq!(a.quads().uid_of(NS, "http://ex.org/x").unwrap(), None);
}

#[test]
fn test_an_empty_batch_applies_nothing() {
    let a = changesets();
    assert!(a.apply_batch(&[]).unwrap().is_empty());
    assert!(a.quads().store().contents().keys().is_empty());
}

#[test]
fn test_a_changeset_assigning_an_iri_an_earlier_one_of_its_batch_assigned_is_aborted() {
    let a = changesets();
    declare(&a, "name", "a property");
    let results = a
        .apply_batch(&[
            changeset(vec![insert("<n> <name> \"first\"")]),
            changeset(vec![insert("<n> <name> \"second\"")]),
            Changeset::new(NS).with(insert("<m> <name> \"third\"")),
        ])
        .unwrap();
    assert!(results[0].is_ok(), "{:?}", results[0]);
    let aborted = results[1].as_ref().unwrap_err();
    assert!(
        matches!(&aborted.0, ChangesetErrorEnum::Quad(q) if matches!(q.0, QuadErrorEnum::Aborted)),
        "{aborted:?}"
    );
    assert_eq!(
        aborted.to_string(),
        "Transaction has been aborted. Please retry"
    );
    assert!(results[2].is_ok(), "{:?}", results[2]);
    assert_eq!(objects(&a, "n", "name"), vec![term("\"first\"")]);
    assert!(is_held_in_default(&a, "m", "name"));
    // A retry finds the UID the first assigned.
    a.apply(&changeset(vec![insert("<n> <name> \"second\"")]))
        .unwrap();
    let mut held = objects(&a, "n", "name");
    held.sort_by_key(|t| t.to_string());
    assert_eq!(held, vec![term("\"first\""), term("\"second\"")]);
}

#[test]
fn test_a_catalog_change_of_an_aborted_changeset_is_not_kept() {
    let a = changesets();
    declare(&a, "name", "a property");
    let results = a
        .apply_batch(&[
            changeset(vec![insert("<n> <name> \"first\"")]),
            // Aborted: `n` was assigned by the first; its default entry of `alias` is not committed.
            changeset(vec![insert("<n> <alias> \"x\"")]),
            changeset(vec![insert("<m> <name> \"third\"")]),
        ])
        .unwrap();
    assert!(
        results[0].is_ok() && results[1].is_err() && results[2].is_ok(),
        "{results:?}"
    );
    let snapshot = a
        .quads()
        .store()
        .snapshot(a.quads().lists().latest_ts())
        .unwrap();
    let stored = ultragraphdb_core_catalog::Catalog::load(&snapshot, NS).unwrap();
    assert_eq!(*a.catalog(NS).unwrap(), stored);
    assert_eq!(a.quads().uid_of(NS, "http://ex.org/alias").unwrap(), None);
}
