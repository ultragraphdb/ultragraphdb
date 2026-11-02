/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_store_posting::PostingError;
use ultragraphdb_tool_bench::{BenchError, BenchErrorEnum};

#[test]
fn test_each_reason_says_what_failed() {
    assert_eq!(
        BenchError::usage("unknown flag --x".to_string()).to_string(),
        "usage: unknown flag --x"
    );
    assert_eq!(
        BenchError::io("no such dir".to_string()).to_string(),
        "i/o: no such dir"
    );
    assert_eq!(
        BenchError::commit("the log is poisoned".to_string()).to_string(),
        "commit: the log is poisoned"
    );
    let store = BenchError::from(StoreError::ts_too_old());
    assert!(matches!(&store.0, BenchErrorEnum::Store(e) if *e == StoreError::ts_too_old()));
    assert_eq!(store.to_string(), "store: Transaction is too old");
    // Dgraph's ErrTsTooOld text, through the applier's error.
    let apply = BenchError::apply(ChangesetError::from(PostingError::ts_too_old()));
    assert_eq!(apply.to_string(), "apply: Transaction is too old");
}

#[test]
fn test_each_variant_is_built_by_its_constructor() {
    assert!(matches!(BenchError::usage("u".into()).0, BenchErrorEnum::Usage(r) if r == "u"));
    assert!(matches!(BenchError::io("i".into()).0, BenchErrorEnum::Io(r) if r == "i"));
    assert!(matches!(BenchError::commit("c".into()).0, BenchErrorEnum::Commit(r) if r == "c"));
    assert!(matches!(
        BenchError::store(StoreError::ts_too_old()).0,
        BenchErrorEnum::Store(e) if e == StoreError::ts_too_old()
    ));
    let changeset = ChangesetError::from(PostingError::ts_too_old());
    assert!(matches!(
        BenchError::apply(changeset.clone()).0,
        BenchErrorEnum::Apply(e) if e == changeset
    ));
}

#[test]
fn test_the_applier_and_the_posting_lists_errors_are_apply_errors() {
    let changeset = ChangesetError::from(PostingError::ts_too_old());
    assert!(matches!(
        BenchError::from(changeset.clone()).0,
        BenchErrorEnum::Apply(e) if e == changeset
    ));
    // A posting list's error reaches the run as the applier's.
    assert!(matches!(
        BenchError::from(PostingError::ts_too_old()).0,
        BenchErrorEnum::Apply(e) if e == changeset
    ));
}

#[test]
fn test_an_io_error_keeps_its_message() {
    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "no such dir");
    let error = BenchError::from(io);
    assert!(matches!(&error.0, BenchErrorEnum::Io(r) if r == "no such dir"));
    assert_eq!(error.to_string(), "i/o: no such dir");
    assert!(error.source().is_none());
}
