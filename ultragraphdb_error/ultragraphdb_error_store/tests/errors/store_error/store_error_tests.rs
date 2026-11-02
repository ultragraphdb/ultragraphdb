/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(StoreError::ts_too_old().0, StoreErrorEnum::TsTooOld);
    assert_eq!(
        StoreError::key_too_large(9, 8, vec![1]).0,
        StoreErrorEnum::KeyTooLarge {
            size: 9,
            limit: 8,
            head: vec![1]
        }
    );
    assert_eq!(
        StoreError::read_failed(vec![2], "r".into()).0,
        StoreErrorEnum::ReadFailed {
            key: vec![2],
            reason: "r".into()
        }
    );
    assert_eq!(
        StoreError::corrupted(vec![3], "c".into()).0,
        StoreErrorEnum::Corrupted {
            key: vec![3],
            reason: "c".into()
        }
    );
    assert_eq!(
        StoreError::write_failed("w".into()).0,
        StoreErrorEnum::WriteFailed("w".into())
    );
    assert_eq!(StoreError::poisoned().0, StoreErrorEnum::Poisoned);
    assert_eq!(
        StoreError::open_failed("o".into()).0,
        StoreErrorEnum::OpenFailed("o".into())
    );
    assert_eq!(
        StoreError::unsupported("temporary keyspaces").0,
        StoreErrorEnum::Unsupported("temporary keyspaces")
    );
    assert_eq!(
        StoreError::unknown_temporary(4).0,
        StoreErrorEnum::UnknownTemporary(4)
    );
    assert_eq!(StoreError::empty_key().0, StoreErrorEnum::EmptyKey);
}

#[test]
fn test_every_classification_and_every_field_tells_errors_apart() {
    // One error per variant, then each with one field changed: no two may compare equal, and a set of every error
    // twice keeps each once, so a caller branching on `.0` or deduplicating errors never merges two failures.
    let errors = [
        StoreError::ts_too_old(),
        StoreError::key_too_large(9, 8, vec![1]),
        StoreError::key_too_large(10, 8, vec![1]),
        StoreError::key_too_large(9, 7, vec![1]),
        StoreError::key_too_large(9, 8, vec![2]),
        StoreError::key_too_large(9, 8, Vec::new()),
        StoreError::read_failed(vec![2], "r".into()),
        StoreError::read_failed(vec![3], "r".into()),
        StoreError::read_failed(vec![2], "s".into()),
        StoreError::corrupted(vec![2], "r".into()),
        StoreError::corrupted(vec![3], "r".into()),
        StoreError::corrupted(vec![2], "s".into()),
        StoreError::write_failed("r".into()),
        StoreError::write_failed(String::new()),
        StoreError::poisoned(),
        StoreError::open_failed("r".into()),
        StoreError::open_failed(String::new()),
        StoreError::unsupported("r"),
        StoreError::unsupported("temporary keyspaces"),
        StoreError::unknown_temporary(0),
        StoreError::unknown_temporary(u64::MAX),
        StoreError::empty_key(),
    ];
    for (i, a) in errors.iter().enumerate() {
        for (j, b) in errors.iter().enumerate() {
            assert_eq!(a == b, i == j, "{a:?} against {b:?}");
        }
    }
    let set: HashSet<StoreError> = errors
        .iter()
        .cloned()
        .chain(errors.iter().cloned())
        .collect();
    assert_eq!(set.len(), errors.len());
}

#[test]
fn test_debug_names_the_variant_and_its_fields() {
    assert_eq!(
        format!("{:?}", StoreError::ts_too_old()),
        "StoreError(TsTooOld)"
    );
    assert_eq!(
        format!("{:?}", StoreError::key_too_large(9, 8, vec![1])),
        "StoreError(KeyTooLarge { size: 9, limit: 8, head: [1] })"
    );
    assert_eq!(
        format!("{:?}", StoreError::unknown_temporary(4)),
        "StoreError(UnknownTemporary(4))"
    );
    assert_eq!(
        format!("{:?}", StoreError::empty_key()),
        "StoreError(EmptyKey)"
    );
}
