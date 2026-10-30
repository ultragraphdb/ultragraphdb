/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_tok::get_tokenizer;

use ultragraphdb_core_keys::{CountDirection, Keyspace};
use ultragraphdb_store_sim::LogCapture;
use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    NS, apply, changesets, counted, declaration, declare, delete, exact_token, finish_builds,
    indexed, insert, keys_of, stored_keys, term_token, uid, write,
};

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
    nearest_with(a, &ultragraphdb_traits_index::Options::new(), query, k)
}

/// The nodes a search of `emb`'s index, opened with `options`, finds nearest `query`.
fn nearest_with(
    a: &crate::utils::changesets::TestChangesets,
    options: &ultragraphdb_traits_index::Options,
    query: &[f32],
    k: usize,
) -> Vec<u64> {
    use ultragraphdb_core_keys::DEFAULT_GRAPH;
    use ultragraphdb_store_hnsw::{PersistentHnsw, QueryCache, VectorKeys, ViTxn};
    use ultragraphdb_traits_index::accept_all;
    let keys = VectorKeys::new(
        crate::utils::changesets::ex("emb").as_str(),
        NS,
        DEFAULT_GRAPH,
        uid(a, "emb"),
    );
    let latest = a.quads().lists().applied_ts();
    let mut request = a.quads().lists().request(latest);
    let txn = ViTxn::new(&mut request);
    PersistentHnsw::<f32>::new(keys, options, 32)
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
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
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

#[test]
fn test_a_count_index_is_built_over_held_edges_and_the_writes_during_its_build() {
    let a = changesets();
    write(
        &a,
        &[
            "<a> <friend> <b>",
            "<a> <friend> <c>",
            "<a> <friend> <d>",
            "<e> <friend> <b>",
        ],
    );
    declare(&a, "friend", "with an exact count index");
    assert!(a.rebuilding());
    // Until the swap, queries read the declaration without the count index being built.
    let interim = a
        .catalog(NS)
        .unwrap()
        .predicate(uid(&a, "friend"))
        .cloned()
        .unwrap();
    assert!(!interim.count_index(), "{interim:?}");
    // Written while the count index is built: the swap's lists and the write's deltas both count it.
    write(&a, &["<f> <friend> <b>"]);
    finish_builds(&a);
    assert!(!a.rebuilding());
    assert_eq!(
        a.catalog(NS).unwrap().predicate(uid(&a, "friend")),
        Some(&declaration("friend", "with an exact count index"))
    );
    let n = |name: &str| uid(&a, name);
    let sorted = |mut v: Vec<u64>| {
        v.sort();
        v
    };
    // Hand-counted: `a` holds 3 objects, `e` and `f` one each; `b` has 3 subjects, `c` and `d` one each.
    assert_eq!(
        counted(&a, "friend", CountDirection::Forward, 3),
        vec![n("a")]
    );
    assert_eq!(
        sorted(counted(&a, "friend", CountDirection::Forward, 1)),
        sorted(vec![n("e"), n("f")])
    );
    assert_eq!(
        counted(&a, "friend", CountDirection::Reverse, 3),
        vec![n("b")]
    );
    assert_eq!(
        sorted(counted(&a, "friend", CountDirection::Reverse, 1)),
        sorted(vec![n("c"), n("d")])
    );
    for count in [2, 4] {
        assert!(
            counted(&a, "friend", CountDirection::Forward, count).is_empty(),
            "{count}"
        );
        assert!(
            counted(&a, "friend", CountDirection::Reverse, count).is_empty(),
            "{count}"
        );
    }
}

#[test]
fn test_a_build_beyond_the_held_bound_merges_every_flush() {
    // 5,001 literals of two words each: 10,002 postings, past the bound of 10,000 held postings, so `common` is
    // written to the temporary keyspace in two deltas, before and after the bound.
    const SUBJECTS: usize = 5_001;
    let a = changesets();
    let changes = (0..SUBJECTS)
        .map(|i| insert(&format!("<s{i}> <name> \"w{i} common\"")))
        .collect();
    apply(&a, changes).unwrap();
    declare(&a, "name", "with `term`");
    assert!(a.rebuilding());
    let capture = LogCapture::new();
    capture.capture(|| finish_builds(&a));
    let built: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|e| e.message() == Some("indexes built into the temporary keyspace"))
        .collect();
    assert_eq!(built.len(), 1);
    // 5,001 subjects of two tokens each: 10,002 postings added. The buffer is written out once it holds 10,000, after
    // the 5,000th subject, so it never holds more.
    assert_eq!(built[0].field("postings"), Some("10002"));
    assert_eq!(built[0].field("postings_held"), Some("10000"));
    assert_eq!(indexed(&a, "name", &term_token("common")).len(), SUBJECTS);
    for i in [0, 4_999, 5_000] {
        assert_eq!(
            indexed(&a, "name", &term_token(&format!("w{i}"))),
            vec![uid(&a, &format!("s{i}"))]
        );
    }
    // One key per word and one for `common`.
    let keys = keys_of(&a, Keyspace::ValueIndex, "name");
    assert_eq!(keys.len(), SUBJECTS + 1);
    // Each list built is written complete, with the discard flag a rollup's complete version carries.
    let contents = a.quads().store().contents();
    for key in &keys {
        assert!(contents.keys()[key].iter().any(|v| v.discard()), "{key:?}");
    }
}

#[test]
fn test_a_build_writes_a_deletion_for_a_key_whose_list_it_leaves_empty() {
    let a = changesets();
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term` and `exact`");
    // Deleted during the build: the maintained `exact` delta empties the list, which the build reads at its start.
    apply(&a, vec![delete("<a> <name> \"big apple\"")]).unwrap();
    finish_builds(&a);
    assert_eq!(
        a.catalog(NS).unwrap().predicate(uid(&a, "name")),
        Some(&declaration("name", "with `term` and `exact`"))
    );
    assert!(indexed(&a, "name", &exact_token("big apple")).is_empty());
    assert!(indexed(&a, "name", &term_token("big")).is_empty());
}

#[test]
fn test_a_vector_deleted_during_its_build_is_dead_after_the_swap() {
    let a = changesets();
    declare(&a, "emb", "single-valued");
    write(
        &a,
        &[
            "<a> <emb> \"[1, 0, 0]\"^^ugdb:float32vector",
            "<b> <emb> \"[0, 1, 0]\"^^ugdb:float32vector",
        ],
    );
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
    apply(
        &a,
        vec![delete("<a> <emb> \"[1, 0, 0]\"^^ugdb:float32vector")],
    )
    .unwrap();
    finish_builds(&a);
    let found = nearest(&a, &[1.0, 0.0, 0.0], 2);
    assert_eq!(found, vec![uid(&a, "b")], "a dead node is not found");
    assert_eq!(
        a.catalog(NS).unwrap().predicate(uid(&a, "emb")),
        Some(&declaration(
            "emb",
            "single-valued with a vector index of dimension 3"
        ))
    );
    // The swap marks the node whose vector the write deleted dead: the dead list holds `a` alone.
    let keys = ultragraphdb_store_hnsw::VectorKeys::new(
        crate::utils::changesets::ex("emb").as_str(),
        NS,
        ultragraphdb_core_keys::DEFAULT_GRAPH,
        uid(&a, "emb"),
    );
    let latest = a.quads().lists().applied_ts();
    let dead = a.quads().lists().read(&keys.dead(), latest).unwrap();
    let value = dead
        .static_value(latest)
        .and_then(|pl| pl.postings().first())
        .map(|p| p.value().to_vec())
        .expect("a dead list");
    assert_eq!(
        ultragraphdb_store_hnsw::parse_edges(&String::from_utf8_lossy(&value)).unwrap(),
        vec![uid(&a, "a")]
    );
}

#[test]
fn test_a_declaration_changing_no_index_builds_nothing() {
    let a = changesets();
    declare(&a, "friend", "with an exact count index and `term`");
    write(&a, &["<a> <friend> <b>", "<a> <friend> \"x\""]);
    // The same declaration again, and one changing the cardinality alone.
    declare(&a, "friend", "with an exact count index and `term`");
    assert!(!a.rebuilding());
    declare(&a, "tag", "with `term`");
    write(&a, &["<a> <tag> \"y\""]);
    declare(&a, "tag", "single-valued with `term`");
    assert!(!a.rebuilding());
    // A tokenizer added to a predicate without a count index builds no count key.
    declare(&a, "tag", "single-valued with `term` and `exact`");
    assert!(a.rebuilding());
    finish_builds(&a);
    assert!(keys_of(&a, Keyspace::CountIndex, "tag").is_empty());
    assert_eq!(indexed(&a, "tag", &exact_token("y")), vec![uid(&a, "a")]);
}

#[test]
fn test_dropping_another_predicate_keeps_the_build() {
    let a = changesets();
    write(&a, &["<a> <name> \"pear\"", "<a> <knows> <b>"]);
    declare(&a, "name", "with `exact`");
    a.drop_predicate(NS, crate::utils::changesets::ex("knows").as_ref())
        .unwrap();
    assert!(a.rebuilding());
    finish_builds(&a);
    assert_eq!(
        indexed(&a, "name", &exact_token("pear")),
        vec![uid(&a, "a")]
    );
}

#[test]
fn test_a_key_the_build_produces_nothing_for_is_deleted_at_the_build_s_timestamp() {
    let a = changesets();
    write(&a, &["<a> <name> \"big apple\"", "<b> <name> \"pear\""]);
    // Declared and deleted in one changeset: the delete is maintained at the change's timestamp, where the build reads
    // `a` holding nothing, so the `exact` key of "big apple" holds a delta the build produces no list for.
    apply(
        &a,
        vec![
            ultragraphdb_core_mapping::Change::Declare(
                ultragraphdb_core_mapping::Declaration::Predicate(declaration(
                    "name",
                    "with `exact`",
                )),
            ),
            delete("<a> <name> \"big apple\""),
        ],
    )
    .unwrap();
    assert!(a.rebuilding());
    finish_builds(&a);
    let key = ultragraphdb_core_keys::QuadKey::ValueIndex {
        namespace: NS,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: uid(&a, "name"),
        tokenizer: 0x02,
        token: b"big apple".to_vec(),
    }
    .to_bytes();
    let contents = a.quads().store().contents();
    let newest = contents.keys()[&key]
        .iter()
        .max_by_key(|v| v.version())
        .unwrap();
    assert!(newest.is_deletion(), "{:?}", contents.keys()[&key]);
    assert_eq!(
        indexed(&a, "name", &exact_token("pear")),
        vec![uid(&a, "b")]
    );
}

#[test]
fn test_a_vector_index_rebuilt_with_other_options_replaces_its_lists() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
    write(
        &a,
        &[
            "<a> <emb> \"[1, 0, 0]\"^^ugdb:float32vector",
            "<b> <emb> \"[0, 1, 0]\"^^ugdb:float32vector",
            "<c> <emb> \"[0, 0, 1]\"^^ugdb:float32vector",
        ],
    );
    let with_levels =
        ultragraphdb_core_mapping::PredicateDeclaration::new(crate::utils::changesets::ex("emb"))
            .with_cardinality(ultragraphdb_core_mapping::Cardinality::Single)
            .with_vector_index(
                ultragraphdb_core_mapping::VectorIndexSpec::new(
                    "hnsw",
                    vec![("maxLevels".to_string(), "4".to_string())],
                )
                .with_dimension(std::num::NonZeroU16::new(3).unwrap()),
            );
    apply(
        &a,
        vec![ultragraphdb_core_mapping::Change::Declare(
            ultragraphdb_core_mapping::Declaration::Predicate(with_levels.clone()),
        )],
    )
    .unwrap();
    assert!(a.rebuilding());
    finish_builds(&a);
    assert_eq!(
        a.catalog(NS).unwrap().predicate(uid(&a, "emb")),
        Some(&with_levels)
    );
    let mut levels = ultragraphdb_traits_index::Options::new();
    levels.set_opt(
        "maxLevels".to_string(),
        ultragraphdb_traits_index::OptionValue::Int(4),
    );
    for (subject, query) in [
        ("a", [1.0, 0.0, 0.0]),
        ("b", [0.0, 1.0, 0.0]),
        ("c", [0.0, 0.0, 1.0]),
    ] {
        assert_eq!(
            nearest_with(&a, &levels, &query, 1),
            vec![uid(&a, subject)],
            "{subject}"
        );
    }
}

#[test]
fn test_a_vector_deleted_before_its_build_is_not_marked_dead() {
    let a = changesets();
    declare(&a, "emb", "single-valued");
    write(
        &a,
        &[
            "<a> <emb> \"[1, 0, 0]\"^^ugdb:float32vector",
            "<b> <emb> \"[0, 1, 0]\"^^ugdb:float32vector",
        ],
    );
    apply(
        &a,
        vec![delete("<a> <emb> \"[1, 0, 0]\"^^ugdb:float32vector")],
    )
    .unwrap();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
    finish_builds(&a);
    let keys = ultragraphdb_store_hnsw::VectorKeys::new(
        crate::utils::changesets::ex("emb").as_str(),
        NS,
        ultragraphdb_core_keys::DEFAULT_GRAPH,
        uid(&a, "emb"),
    );
    // `a` holds no vector at the build's timestamp: the build leaves it out, and no dead list is written.
    assert!(!keys_of(&a, Keyspace::VectorIndex, "emb").contains(&keys.dead()));
    assert_eq!(nearest(&a, &[1.0, 0.0, 0.0], 2), vec![uid(&a, "b")]);
}

#[test]
fn test_a_count_build_lists_no_list_emptied_before_it() {
    let a = changesets();
    write(&a, &["<g> <friend> <h>", "<a> <friend> <b>"]);
    apply(&a, vec![delete("<g> <friend> <h>")]).unwrap();
    declare(&a, "friend", "with an exact count index");
    finish_builds(&a);
    // `g`'s list and `h`'s reverse list are stored and empty: no count key of 0 is built for either.
    for direction in [CountDirection::Forward, CountDirection::Reverse] {
        assert!(
            counted(&a, "friend", direction, 0).is_empty(),
            "{direction:?}"
        );
        assert_eq!(
            counted(&a, "friend", direction, 1).len(),
            1,
            "{direction:?}"
        );
    }
    assert_eq!(keys_of(&a, Keyspace::CountIndex, "friend").len(), 2);
}

/// Two above the watermark: the version the commit step gives the next round here.
fn next_version(a: &crate::utils::changesets::TestChangesets) -> u64 {
    a.quads().lists().applied_ts() + 2
}

/// A round of `changes` executed at the watermark and applied at the next version.
fn round_of(
    a: &crate::utils::changesets::TestChangesets,
    changes: Vec<ultragraphdb_core_mapping::Change>,
) -> u64 {
    let version = next_version(a);
    let executed = a
        .execute(&crate::utils::changesets::changeset(changes))
        .unwrap();
    a.apply_round(vec![(version, executed)]).unwrap();
    version
}

/// An index removed by a round is dropped, and the round's record removed, at the watermark: no version of their own,
/// so the commit step's next version is free (fix 1).
#[test]
fn test_removing_an_index_takes_no_version_of_its_own() {
    use ultragraphdb_core_mapping::{Change, Declaration};
    let a = changesets();
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term`");
    finish_builds(&a);
    assert!(!indexed(&a, "name", &term_token("apple")).is_empty());
    let version = round_of(
        &a,
        vec![Change::Declare(Declaration::Predicate(declaration(
            "name", "",
        )))],
    );
    assert_eq!(
        a.quads().lists().applied_ts(),
        version,
        "the round's version, no other"
    );
    assert!(
        indexed(&a, "name", &term_token("apple")).is_empty(),
        "dropped"
    );
    assert!(
        a.quads().store().rebuild_records().unwrap().is_empty(),
        "removed"
    );
    // The next round takes the next version.
    round_of(&a, vec![insert("<b> <name> \"pear\"")]);
}

/// Sequenced builds (fix 1): the job builds and ends with a swap the commit step gives a version; the build is over
/// only once the swap is written there.
#[test]
fn test_a_sequenced_build_ends_with_a_swap_written_at_its_rounds_version() {
    use ultragraphdb_store_apply::BuildEnd;
    let a = changesets();
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term`");
    assert!(a.rebuilding());
    assert_eq!(a.take_build_end(), None, "the job has not run");
    finish_builds(&a);
    assert!(a.rebuilding(), "built, not swapped");
    assert!(indexed(&a, "name", &term_token("apple")).is_empty());
    let end = a.take_build_end().expect("the job ended");
    assert!(matches!(end, BuildEnd::Swap(_)), "{end:?}");
    assert_eq!(a.take_build_end(), None, "one end a build");
    let swap = a.prepare_build_end(end).unwrap();
    assert_eq!(swap.build_end(), Some(end));
    assert!(!swap.starts_build() && !swap.is_drop());
    let version = next_version(&a);
    a.apply_round(vec![(version, swap)]).unwrap();
    assert_eq!(a.quads().lists().applied_ts(), version);
    assert!(!a.rebuilding());
    assert_eq!(
        indexed(&a, "name", &term_token("apple")),
        vec![uid(&a, "a")]
    );
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
}

/// A sequenced build a drop cancels ends with an undo, which removes its record at its round's version and writes
/// nothing else.
#[test]
fn test_a_sequenced_build_a_drop_cancels_ends_with_an_undo() {
    use ultragraphdb_store_apply::BuildEnd;
    let a = changesets();
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term`");
    let drop = a
        .prepare_drop(
            NS,
            ultragraphdb_store_apply::DropRequest::Predicate(
                crate::utils::changesets::ex("name").as_ref(),
            ),
        )
        .unwrap();
    let version = next_version(&a);
    a.apply_round(vec![(version, drop)]).unwrap();
    assert!(!a.rebuilding(), "the drop cancelled it");
    finish_builds(&a);
    let end = a.take_build_end().expect("the job ended");
    assert!(matches!(end, BuildEnd::Undo(_)), "{end:?}");
    assert!(
        !a.quads().store().rebuild_records().unwrap().is_empty(),
        "kept until the undo"
    );
    let undo = a.prepare_build_end(end).unwrap();
    let version = next_version(&a);
    a.apply_round(vec![(version, undo)]).unwrap();
    assert_eq!(a.quads().lists().applied_ts(), version);
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
}

/// A sequenced build swapped after a drop cancelled it is discarded: its record removed, nothing swapped in.
#[test]
fn test_a_swap_after_a_drop_cancelled_its_build_is_discarded() {
    let a = changesets();
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term`");
    finish_builds(&a);
    let swap = a.prepare_build_end(a.take_build_end().unwrap()).unwrap();
    let drop = a
        .prepare_drop(
            NS,
            ultragraphdb_store_apply::DropRequest::Predicate(
                crate::utils::changesets::ex("name").as_ref(),
            ),
        )
        .unwrap();
    let version = next_version(&a);
    a.apply_round(vec![(version, drop)]).unwrap();
    a.apply_round(vec![(version + 2, swap)]).unwrap();
    assert_eq!(a.quads().lists().applied_ts(), version + 2);
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
    assert!(
        a.catalog(NS).unwrap().predicate(uid(&a, "name")).is_none()
            || indexed(&a, "name", &term_token("apple")).is_empty()
    );
}
