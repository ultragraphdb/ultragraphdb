/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_hnsw::{HnswError, HnswErrorEnum};
use ultragraphdb_store_posting::PostingError;

#[test]
fn test_texts_are_dgraphs() {
    let fetching = HnswError::fetching_posting_list(HnswError::no_value(), "0-a", 5);
    assert_eq!(
        fetching.to_string(),
        "No value found: error fetching posting list; 0-a with uid 5"
    );
    assert_eq!(
        HnswError::nil_vector(Some(fetching.clone())).to_string(),
        "nil vector returned; No value found: error fetching posting list; 0-a with uid 5"
    );
    assert_eq!(
        HnswError::nil_vector(None).to_string(),
        "nil vector returned"
    );
    assert_eq!(
        HnswError::empty_tree(None).to_string(),
        "HNSW tree has no elements"
    );
    assert_eq!(
        HnswError::empty_tree(Some(HnswError::key_not_found())).to_string(),
        "HNSW tree has no elements: Key not found"
    );
    assert_eq!(
        HnswError::different_lengths("dot product").to_string(),
        "can not compute dot product on vectors of different lengths"
    );
    assert_eq!(
        HnswError::cannot_cast("int").to_string(),
        "cannot cast int to SimilarityType"
    );
    assert_eq!(
        HnswError::uint_slice("[a]").to_string(),
        "Cannot convert [a] to uint slice"
    );
}

#[test]
fn test_texts_of_the_cases_dgraph_panics_on() {
    assert_eq!(
        HnswError::float_bits(16, 32).to_string(),
        "a vector index of 32-bit floats cannot be created with 16 float bits"
    );
    assert_eq!(
        HnswError::invalid_parameter("maxLevels", 0).to_string(),
        "the vector index option maxLevels must be at least 1, got 0"
    );
    assert_eq!(
        HnswError::level_missing(7, 2, 1).to_string(),
        "the neighbour lists of uid 7 hold 1 levels, level 2 is read"
    );
    assert_eq!(
        HnswError::no_neighbour().to_string(),
        "a vector index layer search kept no neighbour: its breadth or result count is 0"
    );
}

#[test]
fn test_absence_and_its_wrappers_are_recognised() {
    assert!(HnswError::no_value().is_absence());
    assert!(HnswError::key_not_found().is_absence());
    assert!(!HnswError::no_neighbour().is_absence());
    let fetching = HnswError::fetching_posting_list(HnswError::no_value(), "0-a", 1);
    assert!(fetching.is_fetching_posting_list());
    assert!(HnswError::nil_vector(Some(fetching)).is_fetching_posting_list());
    assert!(!HnswError::nil_vector(None).is_fetching_posting_list());
    assert!(HnswError::nil_vector(None).is_nil_vector());
    let fetching_key = HnswError::fetching_posting_list(HnswError::key_not_found(), "0-a", 1);
    assert!(HnswError::empty_tree(Some(fetching_key.clone())).is_fetching_posting_list());
    assert!(!HnswError::empty_tree(Some(HnswError::key_not_found())).is_fetching_posting_list());
    assert!(!HnswError::nil_vector(Some(HnswError::no_value())).is_fetching_posting_list());
    assert!(!HnswError::no_value().is_fetching_posting_list());
    assert!(
        !fetching_key.is_absence(),
        "a wrapped absence is no absence"
    );
    assert!(!fetching_key.is_nil_vector());
    assert!(!HnswError::empty_tree(None).is_nil_vector());
}

#[test]
fn test_only_an_absent_entry_is_the_empty_index_result() {
    assert!(HnswError::empty_tree(Some(HnswError::key_not_found())).is_empty_index());
    assert!(!HnswError::empty_tree(None).is_empty_index());
    assert!(!HnswError::empty_tree(Some(HnswError::no_value())).is_empty_index());
}

#[test]
fn test_corruption_is_the_stores() {
    let e = HnswError::corrupted(b"k", "short");
    assert_eq!(
        e.0,
        HnswErrorEnum::Posting(PostingError::store(StoreError::corrupted(
            b"k".to_vec(),
            "short".to_string()
        )))
    );
    assert_eq!(e.to_string(), "Corrupted value of key 6b: short");
    let from: HnswError = PostingError::ts_too_old().into();
    assert_eq!(from.to_string(), "Transaction is too old");
    let as_error: &dyn std::error::Error = &from;
    assert!(as_error.source().is_none());
}
