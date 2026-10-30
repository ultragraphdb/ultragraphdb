/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Vectors as a transaction writes them: the refusal of a dimension other than the declared one, with its text, in
//! every graph and for the index's first vector alike; a transaction writing or deleting a vector reads and writes no
//! HNSW list; a vector on a predicate without a vector index.

use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_store_apply::ChangesetErrorEnum;

use crate::utils::changesets::{
    apply, changeset, changesets, counts_of, declare, delete, insert, keys_of, objects, term, uid,
    write,
};
use ultragraphdb_core_keys::CountDirection;

#[test]
fn test_a_vector_of_another_dimension_than_declared_is_refused_naming_both() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
    // The index holds no vector yet: the declaration alone sets the dimension.
    let refused = apply(&a, vec![insert("<b> <emb> \"[1, 2]\"^^ugdb:float32vector")]).unwrap_err();
    assert!(
        matches!(
            refused.0,
            ChangesetErrorEnum::Dimension {
                declared: 3,
                written: 2,
                ..
            }
        ),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "Predicate <http://ex.org/emb> indexes vectors of 3 elements, and a vector of 2 was written"
    );
    // A longer vector is refused as a shorter one is, in a named graph as in the default graph.
    let longer = apply(
        &a,
        vec![insert(
            "<c> <emb> \"[1, 2, 3, 4]\"^^ugdb:float32vector <g1>",
        )],
    )
    .unwrap_err();
    assert!(
        matches!(
            longer.0,
            ChangesetErrorEnum::Dimension {
                declared: 3,
                written: 4,
                ..
            }
        ),
        "{longer:?}"
    );
    // The declared dimension is written, in either graph.
    write(
        &a,
        &[
            "<b> <emb> \"[3, 2, 1]\"^^ugdb:float32vector",
            "<c> <emb> \"[1, 2, 3]\"^^ugdb:float32vector <g1>",
        ],
    );
    assert_eq!(
        objects(&a, "b", "emb"),
        vec![term("\"[3, 2, 1]\"^^ugdb:float32vector")]
    );
}

#[test]
fn test_a_transaction_writing_or_deleting_a_vector_touches_no_hnsw_list() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    write(&a, &["<a> <emb> \"[1, 0]\"^^ugdb:float32vector"]);
    let vector_index =
        |key: &[u8]| matches!(QuadKey::parse(key), Ok((QuadKey::VectorIndex { .. }, _)));
    for change in [
        insert("<b> <emb> \"[0, 1]\"^^ugdb:float32vector"),
        insert("<a> <emb> \"[1, 1]\"^^ugdb:float32vector"),
        delete("<a> <emb> \"[1, 0]\"^^ugdb:float32vector"),
    ] {
        let executed = a.execute(&changeset(vec![change])).unwrap();
        let reads = executed.reads();
        assert!(!reads.lists().iter().any(|k| vector_index(k)), "{reads:?}");
        assert!(
            !reads.statements().iter().any(|(k, _)| vector_index(k)),
            "{reads:?}"
        );
        assert!(
            !reads.ranges().iter().any(|(k, _)| vector_index(k)),
            "{reads:?}"
        );
        let writes = executed.writes();
        assert!(!writes.written_keys().any(vector_index));
        assert!(!writes.entry_keys().any(vector_index));
        // The forward list of `emb` is written: the write is not empty.
        assert!(
            writes
                .written_keys()
                .any(|k| matches!(QuadKey::parse(k), Ok((QuadKey::Forward { .. }, None)))),
            "{writes:?}"
        );
    }
    assert!(keys_of(&a, Keyspace::VectorIndex, "emb").is_empty());
}

#[test]
fn test_a_vector_without_a_vector_index_is_held_and_not_indexed() {
    let a = changesets();
    write(
        &a,
        &[
            "<a> <v> \"[1, 2, 3]\"^^ugdb:float32vector",
            "<b> <v> \"[1, 2]\"^^ugdb:float32vector",
        ],
    );
    assert_eq!(
        objects(&a, "a", "v"),
        vec![term("\"[1, 2, 3]\"^^ugdb:float32vector")]
    );
    assert!(keys_of(&a, Keyspace::VectorIndex, "v").is_empty());
    // A vector takes no reverse entry.
    assert!(keys_of(&a, Keyspace::Reverse, "v").is_empty());
}

/// A vector on a predicate with an exact count index counts its subject's objects, and takes no reverse count, as it
/// takes no reverse entry (quad storage: a vector has no reverse entry): `a` holds one vector, then none.
#[test]
fn test_a_vector_is_counted_forward_and_takes_no_reverse_count() {
    let a = changesets();
    declare(&a, "v", "with an exact count index");
    write(&a, &["<a> <v> \"[1, 2]\"^^ugdb:float32vector"]);
    assert_eq!(
        counts_of(&a, "v", CountDirection::Forward, uid(&a, "a")),
        vec![1]
    );
    let reverse_counts = keys_of(&a, Keyspace::CountIndex, "v")
        .into_iter()
        .filter(|k| {
            matches!(
                QuadKey::parse(k),
                Ok((
                    QuadKey::CountIndex {
                        direction: CountDirection::Reverse,
                        ..
                    },
                    _
                ))
            )
        })
        .count();
    assert_eq!(reverse_counts, 0);
    apply(&a, vec![delete("<a> <v> \"[1, 2]\"^^ugdb:float32vector")]).unwrap();
    assert_eq!(
        counts_of(&a, "v", CountDirection::Forward, uid(&a, "a")),
        Vec::<u32>::new()
    );
}
