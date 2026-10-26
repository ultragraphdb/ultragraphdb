/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vector index's maintenance at its edges: the refusal of another dimension, with its text; the first vector of a
//! graph setting the dimension of that graph alone; a vector on a predicate without a vector index.

use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_apply::ChangesetErrorEnum;

use crate::utils::changesets::{apply, changesets, declare, insert, keys_of, objects, term, write};

#[test]
fn test_a_vector_of_another_dimension_is_refused_naming_both_dimensions() {
    let a = changesets();
    declare(&a, "emb", "single-valued with a vector index");
    write(&a, &["<a> <emb> \"[1, 2, 3]\"^^ugdb:float32vector"]);
    let refused = apply(&a, vec![insert("<b> <emb> \"[1, 2]\"^^ugdb:float32vector")]).unwrap_err();
    assert!(
        matches!(
            refused.0,
            ChangesetErrorEnum::Dimension {
                held: 3,
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
    // The same dimension is written; another graph's index takes its own first dimension.
    write(
        &a,
        &[
            "<b> <emb> \"[3, 2, 1]\"^^ugdb:float32vector",
            "<c> <emb> \"[1, 2]\"^^ugdb:float32vector <g1>",
        ],
    );
    assert_eq!(
        objects(&a, "b", "emb"),
        vec![term("\"[3, 2, 1]\"^^ugdb:float32vector")]
    );
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
