/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(StoreError::TsTooOld().0, StoreErrorEnum::TsTooOld);
    assert_eq!(
        StoreError::KeyTooLarge(9, 8, vec![1]).0,
        StoreErrorEnum::KeyTooLarge {
            size: 9,
            limit: 8,
            head: vec![1]
        }
    );
    assert_eq!(
        StoreError::ReadFailed(vec![2], "r".into()).0,
        StoreErrorEnum::ReadFailed {
            key: vec![2],
            reason: "r".into()
        }
    );
    assert_eq!(
        StoreError::Corrupted(vec![3], "c".into()).0,
        StoreErrorEnum::Corrupted {
            key: vec![3],
            reason: "c".into()
        }
    );
    assert_eq!(
        StoreError::WriteFailed("w".into()).0,
        StoreErrorEnum::WriteFailed("w".into())
    );
    assert_eq!(StoreError::Poisoned().0, StoreErrorEnum::Poisoned);
    assert_eq!(
        StoreError::OpenFailed("o".into()).0,
        StoreErrorEnum::OpenFailed("o".into())
    );
    assert_eq!(
        StoreError::Unsupported("temporary keyspaces").0,
        StoreErrorEnum::Unsupported("temporary keyspaces")
    );
    assert_eq!(
        StoreError::UnknownTemporary(4).0,
        StoreErrorEnum::UnknownTemporary(4)
    );
    assert_eq!(StoreError::EmptyKey().0, StoreErrorEnum::EmptyKey);
}

#[test]
fn test_every_classification_and_every_field_tells_errors_apart() {
    // One error per variant, then each with one field changed: no two may compare equal, and a set of every error
    // twice keeps each once, so a caller branching on `.0` or deduplicating errors never merges two failures.
    let errors = [
        StoreError::TsTooOld(),
        StoreError::KeyTooLarge(9, 8, vec![1]),
        StoreError::KeyTooLarge(10, 8, vec![1]),
        StoreError::KeyTooLarge(9, 7, vec![1]),
        StoreError::KeyTooLarge(9, 8, vec![2]),
        StoreError::KeyTooLarge(9, 8, Vec::new()),
        StoreError::ReadFailed(vec![2], "r".into()),
        StoreError::ReadFailed(vec![3], "r".into()),
        StoreError::ReadFailed(vec![2], "s".into()),
        StoreError::Corrupted(vec![2], "r".into()),
        StoreError::Corrupted(vec![3], "r".into()),
        StoreError::Corrupted(vec![2], "s".into()),
        StoreError::WriteFailed("r".into()),
        StoreError::WriteFailed(String::new()),
        StoreError::Poisoned(),
        StoreError::OpenFailed("r".into()),
        StoreError::OpenFailed(String::new()),
        StoreError::Unsupported("r"),
        StoreError::Unsupported("temporary keyspaces"),
        StoreError::UnknownTemporary(0),
        StoreError::UnknownTemporary(u64::MAX),
        StoreError::EmptyKey(),
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
        format!("{:?}", StoreError::TsTooOld()),
        "StoreError(TsTooOld)"
    );
    assert_eq!(
        format!("{:?}", StoreError::KeyTooLarge(9, 8, vec![1])),
        "StoreError(KeyTooLarge { size: 9, limit: 8, head: [1] })"
    );
    assert_eq!(
        format!("{:?}", StoreError::UnknownTemporary(4)),
        "StoreError(UnknownTemporary(4))"
    );
    assert_eq!(
        format!("{:?}", StoreError::EmptyKey()),
        "StoreError(EmptyKey)"
    );
}
