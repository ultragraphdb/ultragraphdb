/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_tok::get_tokenizer;

use crate::utils::changesets::{NS, changesets, declare, finish_builds, stored_keys, uid, write};

#[test]
fn test_a_swap_deletes_the_keys_of_a_removed_tokenizer() {
    let a = changesets();
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    write(&a, &["<b> <name> \"pear\""]);
    assert!(a.rebuilding());
    finish_builds(&a);
    assert!(!a.rebuilding());
    let p = uid(&a, "name");
    let declared = a.catalog(NS).unwrap().predicate(p).cloned().unwrap();
    assert_eq!(declared.tokenizers(), ["exact"]);
    let term = get_tokenizer("term").unwrap().identifier();
    let left: Vec<_> = stored_keys(&a)
        .into_iter()
        .filter(|k| {
            matches!(QuadKey::parse(k), Ok((QuadKey::ValueIndex { predicate, tokenizer, .. }, _)) if predicate == p && tokenizer == term)
        })
        .collect();
    let contents = a.quads().store().contents();
    for k in &left {
        eprintln!("{k:?} {:?}", contents.keys().get(k));
    }
    assert!(left.is_empty());
}

/// The nodes a search of `emb`'s index in the default graph finds nearest `query`.
fn nearest(a: &crate::utils::changesets::TestChangesets, query: &[f32], k: usize) -> Vec<u64> {
    use ultragraphdb_core_keys::DEFAULT_GRAPH;
    use ultragraphdb_store_hnsw::{PersistentHnsw, QueryCache, VectorKeys, ViTxn};
    use ultragraphdb_traits_index::{Options, accept_all};
    let keys = VectorKeys::new(
        crate::utils::changesets::ex("emb").as_str(),
        NS,
        DEFAULT_GRAPH,
        uid(a, "emb"),
    );
    let latest = a.quads().lists().latest_ts();
    let mut request = a.quads().lists().request(latest);
    let txn = ViTxn::new(&mut request);
    PersistentHnsw::<f32>::new(keys, &Options::new(), 32)
        .unwrap()
        .search(&QueryCache::new(&txn, latest), query, k, accept_all::<f32>)
        .unwrap()
}

#[test]
fn test_a_vector_index_is_built_over_held_vectors_and_the_writes_during_its_build() {
    let a = changesets();
    declare(&a, "emb", "single-valued");
    write(
        &a,
        &[
            "<a> <emb> \"[1, 0, 0]\"^^ugdb:float32vector",
            "<b> <emb> \"[0, 1, 0]\"^^ugdb:float32vector",
        ],
    );
    declare(&a, "emb", "single-valued with a vector index");
    assert!(a.rebuilding());
    // Written while the index is built: the swap inserts it.
    write(&a, &["<c> <emb> \"[0, 0, 1]\"^^ugdb:float32vector"]);
    finish_builds(&a);
    assert!(!a.rebuilding());
    assert!(
        a.catalog(NS)
            .unwrap()
            .predicate(uid(&a, "emb"))
            .is_some_and(|d| d.vector_index().is_some())
    );
    for (subject, query) in [
        ("a", [1.0, 0.0, 0.0]),
        ("b", [0.0, 1.0, 0.0]),
        ("c", [0.0, 0.0, 1.0]),
    ] {
        assert_eq!(nearest(&a, &query, 1), vec![uid(&a, subject)], "{subject}");
    }
}
