/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, Term};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_posting::{Lists, ListsConfig};
use ultragraphdb_store_quads::{QuadStore, QuadWrite};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};

use crate::utils::helpers::{TestQuads, commit, ex, forward, quad, quads, uid, write};

fn literal(text: &str) -> EncodedTerm {
    EncodedTerm::literal(Literal::new_simple_literal(text).as_ref(), |_| {
        Ok::<_, TermError>(0)
    })
    .unwrap()
    .into_term()
}

fn delete(store: &TestQuads, s: &str, p: &str, object: &EncodedTerm) -> Option<Vec<u64>> {
    let mut w = QuadWrite::new(store, 0, store.lists().request(store.lists().fresh_ts()));
    let (s, p) = (uid(store, 0, &ex(s)), uid(store, 0, &ex(p)));
    let deleted = w.delete(0, s, p, object, "x").unwrap();
    commit(store, vec![w]).remove(0).unwrap();
    deleted
}

#[test]
fn test_a_delete_removes_what_is_held_and_keeps_predicates_still_in_use() {
    let store = quads();
    write(
        &store,
        0,
        &[
            quad("a", "nick", Literal::new_simple_literal("x")),
            quad("a", "nick", Literal::new_simple_literal("y")),
        ],
    );
    write(
        &store,
        0,
        &[quad("c", "nick", Literal::new_simple_literal("x"))],
    );
    let (a, nick) = (uid(&store, 0, &ex("a")), uid(&store, 0, &ex("nick")));
    let preds = |subject: u64| {
        store
            .list_terms(
                0,
                &QuadKey::SubjectPredicates {
                    namespace: 0,
                    graph: 0,
                    subject,
                }
                .to_bytes(),
                u64::MAX,
            )
            .unwrap()
    };
    let object_preds = |object: &EncodedTerm| {
        store
            .list_terms(
                0,
                &QuadKey::ObjectPredicates {
                    namespace: 0,
                    graph: 0,
                    object: object.clone(),
                }
                .to_bytes(),
                u64::MAX,
            )
            .unwrap()
    };
    assert_eq!(
        delete(&store, "a", "nick", &literal("z")),
        None,
        "an object not held changes nothing"
    );
    assert!(delete(&store, "a", "nick", &literal("x")).is_some());
    assert_eq!(preds(a), vec![Term::from(ex("nick"))], "a still holds y");
    assert_eq!(
        object_preds(&literal("x")),
        vec![Term::from(ex("nick"))],
        "c still points at x"
    );
    delete(&store, "a", "nick", &literal("y"));
    assert!(preds(a).is_empty());
    assert!(object_preds(&literal("y")).is_empty());
    let stats = store.statistics(0, 0, nick, u64::MAX).unwrap();
    assert_eq!(stats.statements(), 1);
}

#[test]
fn test_colliding_fingerprints_keep_both_literals_and_deleting_one_keeps_the_other() {
    fn constant(_: &[u8]) -> u64 {
        42
    }
    let lists = Lists::new(
        SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new()),
        SimListCache::new(16, Seed::new(1).stream(Stream::Cache)),
        ListsConfig::default(),
    );
    let store = QuadStore::open(lists).unwrap().with_fingerprint(constant);
    write(
        &store,
        0,
        &[
            quad("a", "name", Literal::new_simple_literal("A")),
            quad("a", "name", Literal::new_simple_literal("B")),
        ],
    );
    let key = forward(&store, 0, 0, &ex("a"), &ex("name"));
    store.lists().rollup_key(&key).unwrap();
    let mut held = store.list_terms(0, &key, u64::MAX).unwrap();
    held.sort_by_key(Term::to_string);
    assert_eq!(
        held,
        vec![
            Literal::new_simple_literal("A").into(),
            Literal::new_simple_literal("B").into()
        ]
    );
    delete(&store, "a", "name", &literal("A"));
    assert_eq!(
        store.list_terms(0, &key, u64::MAX).unwrap(),
        vec![Term::from(Literal::new_simple_literal("B"))]
    );
}
