/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_hnsw::{HnswError, HnswErrorEnum};
use ultragraphdb_store_posting::PostingError;

#[test]
fn test_texts_are_dgraphs() {
    let fetching = HnswError::FetchingPostingList(HnswError::NoValue(), "0-a", 5);
    assert_eq!(
        fetching.to_string(),
        "No value found: error fetching posting list; 0-a with uid 5"
    );
    assert_eq!(
        HnswError::NilVector(Some(fetching.clone())).to_string(),
        "nil vector returned; No value found: error fetching posting list; 0-a with uid 5"
    );
    assert_eq!(
        HnswError::NilVector(None).to_string(),
        "nil vector returned"
    );
    assert_eq!(
        HnswError::EmptyTree(None).to_string(),
        "HNSW tree has no elements"
    );
    assert_eq!(
        HnswError::EmptyTree(Some(HnswError::KeyNotFound())).to_string(),
        "HNSW tree has no elements: Key not found"
    );
    assert_eq!(
        HnswError::DifferentLengths("dot product").to_string(),
        "can not compute dot product on vectors of different lengths"
    );
    assert_eq!(
        HnswError::CannotCast("int").to_string(),
        "cannot cast int to SimilarityType"
    );
    assert_eq!(
        HnswError::UintSlice("[a]").to_string(),
        "Cannot convert [a] to uint slice"
    );
}

#[test]
fn test_texts_of_the_cases_dgraph_panics_on() {
    assert_eq!(
        HnswError::FloatBits(16, 32).to_string(),
        "a vector index of 32-bit floats cannot be created with 16 float bits"
    );
    assert_eq!(
        HnswError::InvalidParameter("maxLevels", 0).to_string(),
        "the vector index option maxLevels must be at least 1, got 0"
    );
    assert_eq!(
        HnswError::LevelMissing(7, 2, 1).to_string(),
        "the neighbour lists of uid 7 hold 1 levels, level 2 is read"
    );
    assert!(
        HnswError::NoNeighbour()
            .to_string()
            .contains("kept no neighbour")
    );
}

#[test]
fn test_absence_and_its_wrappers_are_recognised() {
    assert!(HnswError::NoValue().is_absence());
    assert!(HnswError::KeyNotFound().is_absence());
    assert!(!HnswError::NoNeighbour().is_absence());
    let fetching = HnswError::FetchingPostingList(HnswError::NoValue(), "0-a", 1);
    assert!(fetching.is_fetching_posting_list());
    assert!(HnswError::NilVector(Some(fetching)).is_fetching_posting_list());
    assert!(!HnswError::NilVector(None).is_fetching_posting_list());
    assert!(HnswError::NilVector(None).is_nil_vector());
}

#[test]
fn test_only_an_absent_entry_is_the_empty_index_result() {
    assert!(HnswError::EmptyTree(Some(HnswError::KeyNotFound())).is_empty_index());
    assert!(!HnswError::EmptyTree(None).is_empty_index());
    assert!(!HnswError::EmptyTree(Some(HnswError::NoValue())).is_empty_index());
}

#[test]
fn test_corruption_is_the_stores() {
    let e = HnswError::corrupted(b"k", "short");
    assert_eq!(
        e.0,
        HnswErrorEnum::Posting(PostingError::Store(StoreError::Corrupted(
            b"k".to_vec(),
            "short".to_string()
        )))
    );
    assert_eq!(e.to_string(), "Corrupted value of key 6b: short");
    let from: HnswError = PostingError::NoValue().into();
    assert_eq!(from.to_string(), "No value found");
    let e: &dyn std::error::Error = &from;
    assert!(e.source().is_none());
}
