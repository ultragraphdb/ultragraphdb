/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;
use ultragraphdb_tool_bdd::TestResult;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};

#[test]
fn test_texts_are_dgraphs() -> TestResult {
    let cases = [
        (
            PostingError::invalid_key(),
            "cannot read posting list using multi-part list key",
        ),
        (PostingError::ts_too_old(), "Transaction is too old"),
        (
            PostingError::read_below_min_ts(3, 5, b"k\x00".to_vec()),
            "readTs: 3 less than minTs: 5 for key: \"k\\x00\"",
        ),
        (
            PostingError::delete_below_min_ts(3, 5),
            "deleteBelowTs (3) must be greater than the minTs in the list (5)",
        ),
        (
            PostingError::high_priority_op(),
            "Cancelled rollup to make way for high priority operation",
        ),
        (
            PostingError::missing_part(vec![0x04, 0xab]),
            "could not read list part with key 04ab: Key not found",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
    Ok(())
}

#[test]
fn test_meta_texts_dump_the_key() -> TestResult {
    let dump = "00000000  6b 65 79                                          |key|\n";
    assert_eq!(
        PostingError::schema_posting(b"key".to_vec()).to_string(),
        format!("Trying to read schema in ReadPostingList for key: {dump}")
    );
    assert_eq!(
        PostingError::unexpected_meta(0x20, b"key".to_vec()).to_string(),
        format!("Unexpected meta: 32 for key: {dump}")
    );
    Ok(())
}

#[test]
fn test_store_errors_carry_through_and_a_key_of_no_list_is_named() -> TestResult {
    let error: PostingError = StoreError::ts_too_old().into();
    assert_eq!(error.0, PostingErrorEnum::Store(StoreError::ts_too_old()));
    assert_eq!(error.to_string(), "Transaction is too old");
    let not_a_list = PostingError::not_a_list(b"key");
    assert_eq!(
        not_a_list.0,
        PostingErrorEnum::NotAList {
            key: b"key".to_vec()
        }
    );
    // The key as Go's hex.Dump prints it, as in the meta texts above.
    let dump = "00000000  6b 65 79                                          |key|\n";
    assert_eq!(
        not_a_list.to_string(),
        format!("Key {dump} is not a posting list's")
    );
    Ok(())
}

#[test]
fn test_a_refused_value_is_the_stores_corruption_naming_its_key() -> TestResult {
    let error = PostingError::corrupted(b"\x01\x02", "bad");
    assert!(matches!(
        &error.0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { key, .. })) if key == b"\x01\x02"
    ));
    assert_eq!(error.to_string(), "Corrupted value of key 0102: bad");
    assert!(error.source().is_none());
    Ok(())
}

/// A round's version at or below the one before it is refused naming both.
#[test]
fn test_a_stale_version_names_both_versions() -> TestResult {
    let error = PostingError::stale_version(12, 14);
    assert_eq!(
        error.0,
        PostingErrorEnum::StaleVersion {
            version: 12,
            after: 14
        }
    );
    assert_eq!(error.to_string(), "commit version 12 is not above 14");
    Ok(())
}
