/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};

#[test]
fn test_texts_are_dgraphs() {
    let cases = [
        (
            PostingError::InvalidKey(),
            "cannot read posting list using multi-part list key",
        ),
        (PostingError::TsTooOld(), "Transaction is too old"),
        (
            PostingError::ReadBelowMinTs(3, 5, b"k\x00".to_vec()),
            "readTs: 3 less than minTs: 5 for key: \"k\\x00\"",
        ),
        (
            PostingError::DeleteBelowMinTs(3, 5),
            "deleteBelowTs (3) must be greater than the minTs in the list (5)",
        ),
        (
            PostingError::HighPriorityOp(),
            "Cancelled rollup to make way for high priority operation",
        ),
        (
            PostingError::MissingPart(vec![0x04, 0xab]),
            "could not read list part with key 04ab: Key not found",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_meta_texts_dump_the_key() {
    let dump = "00000000  6b 65 79                                          |key|\n";
    assert_eq!(
        PostingError::SchemaPosting(b"key".to_vec()).to_string(),
        format!("Trying to read schema in ReadPostingList for key: {dump}")
    );
    assert_eq!(
        PostingError::UnexpectedMeta(0x20, b"key".to_vec()).to_string(),
        format!("Unexpected meta: 32 for key: {dump}")
    );
}

#[test]
fn test_store_errors_carry_through_and_a_key_of_no_list_is_named() {
    let error: PostingError = StoreError::TsTooOld().into();
    assert_eq!(error.0, PostingErrorEnum::Store(StoreError::TsTooOld()));
    assert_eq!(error.to_string(), "Transaction is too old");
    let error = PostingError::NotAList(b"key");
    assert_eq!(
        error.0,
        PostingErrorEnum::NotAList {
            key: b"key".to_vec()
        }
    );
    // The key as Go's hex.Dump prints it, as in the meta texts above.
    let dump = "00000000  6b 65 79                                          |key|\n";
    assert_eq!(
        error.to_string(),
        format!("Key {dump} is not a posting list's")
    );
}

#[test]
fn test_a_refused_value_is_the_stores_corruption_naming_its_key() {
    let error = PostingError::corrupted(b"\x01\x02", "bad");
    assert!(matches!(
        &error.0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { key, .. })) if key == b"\x01\x02"
    ));
    assert_eq!(error.to_string(), "Corrupted value of key 0102: bad");
    assert!(error.source().is_none());
}

/// A round's version at or below the one before it is refused naming both.
#[test]
fn test_a_stale_version_names_both_versions() {
    let error = PostingError::StaleVersion(12, 14);
    assert_eq!(
        error.0,
        PostingErrorEnum::StaleVersion {
            version: 12,
            after: 14
        }
    );
    assert_eq!(error.to_string(), "commit version 12 is not above 14");
}
